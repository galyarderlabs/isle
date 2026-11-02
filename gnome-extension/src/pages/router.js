// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 zickrian

// Router: the LLM gateway's own usage — requests, tokens and the cost it estimates — for the
// period Settings picked: a summary card, the busiest models as bars, and the last requests.

import Clutter from 'gi://Clutter';
import GObject from 'gi://GObject';
import Pango from 'gi://Pango';
import St from 'gi://St';

import {VERTICAL, px} from '../compat.js';
import {icon, label} from '../widgets.js';
import {Page} from './page.js';

/** The periods the header offers, in order; Settings' own words. */
const PERIODS = ['today', '24h', '7d'];
/** How each period reads on the header's toggle. */
const PERIOD_LABELS = {today: 'Today', '24h': '24h', '7d': '7d'};
/** The most models the bars list. */
const TOP_MODELS = 5;

/** Compact token counts: 1.2M, 845K, 900. */
function tokens(count) {
    if (count >= 1_000_000)
        return `${(count / 1_000_000).toFixed(1).replace(/\.0$/, '')}M`;
    if (count >= 1000)
        return `${Math.round(count / 1000)}K`;
    return `${count ?? 0}`;
}

/** A relative time: "2m ago", "3h ago". */
function ago(atMs) {
    const seconds = Math.max(0, (Date.now() - atMs) / 1000);
    if (seconds < 60)
        return 'now';
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60)
        return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24)
        return `${hours}h ago`;
    return `${Math.floor(hours / 24)}d ago`;
}

/** Hide provider routing IDs when they are appended to an otherwise useful model name. */
function displayModel(model) {
    return model
        .replace(/\s*\(openai-compatible[^)]*\)$/i, '')
        .replace(/-openai-compatible-chat-[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i, '');
}

export const RouterPage = GObject.registerClass(
class RouterPage extends Page {
    /** `height` is the expanded island's page height. */
    _init(daemon, settings, {height}) {
        super._init(daemon, {
            style_class: 'isle-page isle-router',
            layout_manager: new Clutter.BinLayout(),
            height,
            reactive: true,
        });
        this.name = 'router';
        this._settings = settings;
        this._state = null;

        const layout = new St.BoxLayout({...VERTICAL, style_class: 'isle-router-layout',
            x_expand: true, y_expand: true});

        const heading = new St.BoxLayout({style_class: 'isle-router-heading'});
        heading.add_child(new St.Icon({gicon: icon('network-router-symbolic'), icon_size: 16}));
        heading.add_child(label('Router', 'isle-router-title'));
        this._statusLine = label('Waiting for gateway', 'isle-router-status', {width: px(190)});
        this._statusLine.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        heading.add_child(this._statusLine);
        // The period toggle in the header, kept in step with Settings.
        this._periodToggle = new St.BoxLayout({style_class: 'isle-router-periods'});
        this._periodToggle.x_expand = true;
        this._periodToggle.x_align = Clutter.ActorAlign.END;
        this._periodButtons = {};
        for (const period of PERIODS) {
            const button = new St.Button({style_class: 'isle-router-period', can_focus: true,
                label: PERIOD_LABELS[period], accessible_name: `Router period ${PERIOD_LABELS[period]}`});
            button.connect('clicked', () => this._setPeriod(period));
            this._periodToggle.add_child(button);
            this._periodButtons[period] = button;
        }
        heading.add_child(this._periodToggle);
        layout.add_child(heading);

        // Four compact metric cards give the totals a clear order without another run-on line.
        const metrics = new St.BoxLayout({style_class: 'isle-router-metrics', x_expand: true});
        this._metricValues = {};
        for (const [key, title] of Object.entries({
            requests: 'Requests',
            input: 'Tokens in',
            output: 'Tokens out',
            cost: 'Est. cost',
        })) {
            const card = new St.BoxLayout({...VERTICAL, style_class: 'isle-router-metric', x_expand: true});
            card.add_child(label(title, 'isle-router-metric-label'));
            const value = label('—', 'isle-router-metric-value');
            card.add_child(value);
            this._metricValues[key] = value;
            metrics.add_child(card);
        }
        layout.add_child(metrics);

        // Models and recent requests sit side by side; scroll stays inside the data area.
        const content = new St.BoxLayout({style_class: 'isle-router-content', x_expand: true, y_expand: true});
        this._models = new St.BoxLayout({...VERTICAL, style_class: 'isle-router-section isle-router-models',
            x_expand: true, y_expand: true});
        this._models.add_child(label('Top models', 'isle-router-section-title'));
        content.add_child(this._models);
        this._recent = new St.BoxLayout({...VERTICAL, style_class: 'isle-router-section isle-router-recent',
            x_expand: true, y_expand: true});
        this._recent.add_child(label('Recent requests', 'isle-router-section-title'));
        content.add_child(this._recent);
        this._scroll = new St.ScrollView({
            style_class: 'isle-router-scroll',
            hscrollbar_policy: St.PolicyType.NEVER,
            // Scrolls with the wheel or touchpad; no scrollbar over the list.
            vscrollbar_policy: St.PolicyType.EXTERNAL,
            x_expand: true,
            y_expand: true,
        });
        this._scroll.set_child(content);
        layout.add_child(this._scroll);

        this.add_child(layout);
        this._syncPeriodButtons();
        this.watch(['router'], () => this._update());
        this._settingsChanged = [
            settings.connect('changed::router-period', () => this._syncPeriodButtons()),
            settings.connect('changed::router-enabled', () => this._syncAvailability()),
        ];
        this.connect('destroy', () => this._settingsChanged.forEach(id => settings.disconnect(id)));
        this._update();
    }

    /** As isled last read it. */
    get _router() {
        return this.daemon.state.router;
    }

    /** The page stays available while enabled, so it can explain an offline or unconfigured gateway. */
    get available() {
        return this._settings.get_boolean('router-enabled');
    }

    /** Has isled poll while the page is on screen. */
    onActivate() {
        this.daemon.attemptQuietly('router', 'SetWatching', true);
        this.daemon.attemptQuietly('router', 'Refresh');
    }

    onDeactivate() {
        // The chip keeps its own watch; the page's ends with it.
        this.daemon.attemptQuietly('router', 'SetWatching', this._settings.get_boolean('router-notch'));
    }

    /** Lays the numbers out again from the daemon's last reading, at most once a second. */
    _update() {
        const router = this._router;
        const key = JSON.stringify(router);
        this._syncAvailability();
        if (key === this._key)
            return;
        this._key = key;
        const stats = router?.stats ?? {};
        const connected = router?.status === 'connected';
        this._metricValues.requests.text = connected ? Number(stats.requests ?? 0).toLocaleString() : '—';
        this._metricValues.input.text = connected ? tokens(stats.prompt_tokens) : '—';
        this._metricValues.output.text = connected ? tokens(stats.completion_tokens) : '—';
        this._metricValues.cost.text = connected ? `$${(stats.cost ?? 0).toFixed(2)}` : '—';
        const statusText = {
            connected: `Connected · ${router.version}`,
            outdated: 'Update your gateway first',
            unauthorized: 'Authentication failed · check LLM Router settings',
            unreachable: 'Gateway offline · check its URL and make sure it is running',
            'invalid-response': 'Proxy connected, but its usage response is unsupported',
            'needs-local-setup': 'Local 9Router token not found · install and run 9Router on this computer',
            'needs-credentials': 'Add the password or proxy API key in LLM Router settings',
            'needs-configuration': 'Add the Remote base URL in LLM Router settings',
            'keyring-unavailable': 'GNOME Keyring is locked or unavailable',
            'https-required': 'Remote credentials require HTTPS (HTTP works on this computer only)',
        }[router?.status] ?? 'Waiting for the gateway · configure it in LLM Router settings';
        this._statusLine.text = statusText;
        this._statusLine.accessible_name = statusText;
        this._statusLine.set_style_class_name(`isle-router-status ${connected ? 'connected' : 'offline'}`);
        this._buildModels(stats.by_model ?? []);
        this._buildRecent(stats.recent ?? []);
    }

    _syncAvailability() {
        const available = this.available;
        if (available === this._wasAvailable)
            return;
        this._wasAvailable = available;
        this.emit('availability-changed');
    }

    /** The period Settings picked, on the header's toggle. */
    _syncPeriodButtons() {
        const picked = this._settings.get_string('router-period');
        for (const period of PERIODS) {
            if (period === picked)
                this._periodButtons[period].add_style_class_name('picked');
            else
                this._periodButtons[period].remove_style_class_name('picked');
        }
    }

    /** The header's period toggle changes both the page and the notch chip. */
    _setPeriod(period) {
        this._settings.set_string('router-period', period);
        this._syncPeriodButtons();
        this.daemon.attemptQuietly('router', 'Refresh');
    }

    /** Rebuilds the top models' bars with compact numbers. */
    _buildModels(byModel) {
        for (const child of this._models.get_children().slice(1))
            child.destroy();
        const top = byModel.slice(0, TOP_MODELS);
        if (!top.length) {
            this._models.add_child(label('No model activity yet', 'isle-router-empty'));
            return;
        }
        const widest = Math.max(1, ...top.map(usage => usage.tokens));
        for (const usage of top) {
            const row = new St.BoxLayout({style_class: 'isle-router-model', x_expand: true});
            const name = label(displayModel(usage.model), 'isle-router-model-name', {
                width: px(176),
                accessible_name: usage.model,
            });
            name.clutter_text.ellipsize = Pango.EllipsizeMode.END;
            row.add_child(name);
            const track = new St.Widget({style_class: 'isle-router-model-track', x_expand: true});
            const fill = new St.Widget({style_class: 'isle-router-model-fill'});
            fill.width = Math.max(px(2), Math.round(px(100) * usage.tokens / widest));
            track.add_child(fill);
            row.add_child(track);
            row.add_child(label(tokens(usage.tokens), 'isle-router-model-tokens'));
            this._models.add_child(row);
        }
    }

    /** Rebuilds the last requests: model, in→out tokens, ok/error, relative time. */
    _buildRecent(recent) {
        for (const child of this._recent.get_children().slice(1))
            child.destroy();
        if (!recent.length) {
            this._recent.add_child(label('No recent requests', 'isle-router-empty'));
            return;
        }
        for (const request of recent.slice(0, 5)) {
            const row = new St.BoxLayout({style_class: 'isle-router-request', x_expand: true});
            const name = label(displayModel(request.model), 'isle-router-request-model', {
                width: px(150),
                accessible_name: request.model,
            });
            name.clutter_text.ellipsize = Pango.EllipsizeMode.END;
            row.add_child(name);
            row.add_child(label(`${tokens(request.prompt_tokens)} → ${tokens(request.completion_tokens)}`,
                'isle-router-request-tokens'));
            row.add_child(label(request.ok ? 'OK' : 'ERR',
                `isle-router-request-state${request.ok ? '' : ' error'}`));
            row.add_child(label(request.at_ms ? ago(request.at_ms) : '', 'isle-router-request-time'));
            this._recent.add_child(row);
        }
    }
});
