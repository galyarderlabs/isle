// Copyright (C) 2026 zickrian

//! GSConnect's paired devices on the session bus: their battery, read when
//! GSConnect says a device changed, never on a timer. With GSConnect missing
//! or no device paired, the state stays empty and nothing is published.
//!
//! GSConnect's D-Bus surface (src/service/manager.js): one object per device
//! under an `ObjectManager`, its `Device` interface holding `Connected`,
//! `Name` and `Type`. The battery is not a property there; it is the state of
//! the device's `battery` action, exported over `org.gtk.Actions`.

use std::collections::HashMap;

use futures_util::StreamExt;
use zbus::object_server::InterfaceRef;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Signature, Value as ZValue};
use zbus::{Connection, MatchRule, MessageStream, message::Type as MessageType};

use super::{Device, SystemEvent, SystemService, publish};

const SERVICE: &str = "org.gnome.Shell.Extensions.GSConnect";
const DEVICE: &str = "org.gnome.Shell.Extensions.GSConnect.Device";
/// The object manager's root, where the devices hang beneath.
const MANAGER: &str = "/org/gnome/Shell/Extensions/GSConnect";

/// A battery at or under this is low; at 100 it is full. Each crossing is one alert, not one per report.
const LOW: i16 = 15;

/// Follows GSConnect: each connected device and its battery, published to the state as it changes.
pub async fn watch(session: Connection, iface: InterfaceRef<SystemService>) -> anyhow::Result<()> {
    // The object manager's own signals say a device came or went; each device's property
    // changes say one that stayed changed what it shows. Both re-read the list afresh, so
    // no list of properties is kept in step (the battery is read anew with each).
    let manager = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(SERVICE)?
        .interface("org.freedesktop.DBus.ObjectManager")?
        .build();
    let mut manager = MessageStream::for_match_rule(manager, &session, None).await?;
    let properties = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(SERVICE)?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .arg(0, DEVICE)?
        .build();
    let mut properties = MessageStream::for_match_rule(properties, &session, None).await?;

    // Devices that were already paired when Isle started, before their first change is an event.
    let mut before: Vec<Device> = read_devices(&session).await;
    publish(&iface, |state| state.devices = before.clone(), None).await;

    loop {
        let changed = tokio::select! {
            message = manager.next() => {
                let Some(message) = message else { return Ok(()) };
                message.is_ok()
            }
            message = properties.next() => {
                let Some(Ok(_)) = message else { continue };
                true
            }
        };
        if !changed {
            continue;
        }
        // Any device change re-reads the list: which device changed matters less
        // than the list being right, and one read costs no more than a diff.
        let after = read_devices(&session).await;
        let events = changes(&before, &after);
        before.clone_from(&after);
        publish(
            &iface,
            |state| state.devices = after,
            events.first().cloned(),
        )
        .await;
        for event in events.into_iter().skip(1) {
            publish(&iface, |_| (), Some(event)).await;
        }
    }
}

/// What changed between two reads of the device list, as events worth an alert: a device coming
/// or going, and a battery crossing a line (low under [`LOW`], full at 100). Each crossing alerts
/// once, by remembering the side it came from.
fn changes(before: &[Device], after: &[Device]) -> Vec<SystemEvent> {
    let mut events = Vec::new();
    for device in after {
        let was = before.iter().find(|d| d.id == device.id);
        // Newly connected: one event, whatever its battery says.
        let Some(was) = was else {
            events.push(SystemEvent::Device {
                connected: true,
                name: device.name.clone(),
            });
            continue;
        };
        let Some(level) = device.battery else {
            continue;
        };
        let was_level = was.battery;
        // Only a crossing alerts: into low from above it, to full from below it.
        let into_low = level <= LOW as u8 && was_level.is_some_and(|was| was > LOW as u8);
        let to_full = level == 100 && was_level.is_some_and(|was| was < 100);
        if into_low || to_full {
            events.push(SystemEvent::DeviceBattery {
                low: into_low,
                name: device.name.clone(),
                percent: level,
            });
        }
    }
    for device in before {
        if !after.iter().any(|d| d.id == device.id) {
            events.push(SystemEvent::Device {
                connected: false,
                name: device.name.clone(),
            });
        }
    }
    events
}

/// Every connected device, as the object manager reports them: one object per
/// device, its `Device` interface holding the name and type.
async fn read_devices(session: &Connection) -> Vec<Device> {
    let reply = session
        .call_method(
            Some(SERVICE),
            MANAGER,
            Some("org.freedesktop.DBus.ObjectManager"),
            "GetManagedObjects",
            &(),
        )
        .await;
    if let Err(err) = &reply {
        // The service away is the common case (GSConnect not installed); a bus error with it
        // running is what the user needs to see when a connected phone does not show up.
        log::debug!("GSConnect devices unreadable: {err}");
    }
    let reply = reply.ok();
    let Some(reply) = reply else {
        return Vec::new();
    };
    let body = reply.body();
    let objects = body
        .deserialize::<HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>>()
        .ok();
    let Some(objects) = objects else {
        log::debug!("GSConnect managed objects could not be deserialized");
        return Vec::new();
    };
    let mut devices = Vec::new();
    for (path, interfaces) in objects {
        let path = path.to_string();
        // One device that cannot be read must not cost us the others.
        if let Some(device) = read_device(session, &path, interfaces.get(DEVICE)).await {
            log::debug!(
                "GSConnect device on show: {} ({})",
                device.name,
                device.kind
            );
            devices.push(device);
        } else if interfaces.get(DEVICE).is_some() {
            log::debug!("GSConnect device not read: {path}");
        }
    }
    devices.sort_by(|a, b| a.id.cmp(&b.id));
    devices
}

/// One device's name and type from its object-manager entry; only while GSConnect reports it
/// connected. The battery is read afresh from its action state, for the level and charge.
async fn read_device(
    session: &Connection,
    path: &str,
    device: Option<&HashMap<String, OwnedValue>>,
) -> Option<Device> {
    let device = device?;
    // Only a connected device shows; a paired-but-disconnected phone is silence, not noise.
    if !device
        .get("Connected")
        .and_then(bool_property)
        .unwrap_or(false)
    {
        return None;
    }
    let (battery, charging) = read_battery(session, path).await;
    Some(Device {
        id: path.to_owned(),
        name: device
            .get("Name")
            .and_then(string_property)
            .unwrap_or_else(|| "Device".to_owned()),
        kind: device
            .get("Type")
            .and_then(string_property)
            .unwrap_or_else(|| "smartphone".to_owned()),
        battery,
        charging,
    })
}

/// A property value in `a{sv}`: unwrap the variant, then read a boolean.
fn bool_property(value: &OwnedValue) -> Option<bool> {
    bool::try_from(owned_contents(value)?).ok()
}

/// A property value in `a{sv}`: unwrap the variant, then read a string.
fn string_property(value: &OwnedValue) -> Option<String> {
    String::try_from(owned_contents(value)?).ok()
}

/// Takes ownership of a D-Bus value and unwraps any variant layers around it.
fn owned_contents(value: &OwnedValue) -> Option<ZValue<'static>> {
    let mut value = ZValue::from(value.try_clone().ok()?);
    loop {
        match value {
            ZValue::Value(inner) => value = *inner,
            _ => return Some(value),
        }
    }
}

/// The device's battery, as the state of its `battery` action over `org.gtk.Actions`
/// (`DescribeAll`): `a{s(bgav)}`. Its state array holds a `(bsii)` value — charging,
/// icon name, level (-1 when unknown), and time remaining.
async fn read_battery(session: &Connection, path: &str) -> (Option<u8>, bool) {
    let actions = session
        .call_method(
            Some(SERVICE),
            path,
            Some("org.gtk.Actions"),
            "DescribeAll",
            &(),
        )
        .await
        .ok()
        .and_then(|reply| {
            reply
                .body()
                .deserialize::<HashMap<String, (bool, Signature, Vec<OwnedValue>)>>()
                .ok()
        });
    let Some(actions) = actions else {
        return (None, false);
    };
    let Some((_, _, values)) = actions.get("battery") else {
        return (None, false);
    };
    let state = values.iter().find_map(|value| {
        let value = owned_contents(value)?;
        <(bool, String, i32, i32)>::try_from(value).ok()
    });
    let Some((charging, _icon, level, _time)) = state else {
        log::debug!("GSConnect battery action has no readable state");
        return (None, false);
    };
    (
        u8::try_from(level).ok().filter(|level| *level <= 100),
        charging,
    )
}
