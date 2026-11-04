// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 zickrian

// notchfolkd answers only GNOME Shell for app usage and the clipboard's full texts (see
// crates/notchfolk-daemon/src/caller.rs), and the MCP server is a program of its own. This exports
// the three of those the MCP tools need on Shell's own bus name: the call below comes from
// Shell, notchfolkd lets it through, and the MCP scopes on the server still decide who may ask.

import Gio from 'gi://Gio';

const NAME = 'io.galyarderlabs.notchfolk.Shell1';
const PATH = '/io/github/galyarderlabs/notchfolk/Shell';

const XML = `<node><interface name="${NAME}">
    <method name="GetAppStats"><arg type="s" direction="out"/></method>
    <method name="SearchClipboard"><arg name="query" type="s" direction="in"/><arg type="at" direction="out"/></method>
    <method name="GetClipboardText"><arg name="id" type="t" direction="in"/><arg type="s" direction="out"/></method>
</interface></node>`;

export class ShellProxy {
    /** `connection` is the session bus unless a test brings its own. */
    constructor(daemon, connection = Gio.DBus.session) {
        this._daemon = daemon;
        this._exported = Gio.DBusExportedObject.wrapJSObject(XML, this);
        this._exported.export(connection, PATH);
        this._connection = connection;
    }

    /** Takes Shell's name off the bus again; notchfolkd's rules need no change of their own. */
    destroy() {
        this._exported.unexport(this._connection);
    }

    /** App usage as notchfolkd's JSON, most used first. */
    async GetAppStats() {
        return (await this._read('appUsage', 'GetStats'))[0];
    }

    /** The ids of the clipboard items holding `query`, newest first. */
    async SearchClipboard(query) {
        return (await this._read('clipboard', 'Search', query))[0];
    }

    /** One clipboard item's full text: what would go back on the clipboard. */
    async GetClipboardText(id) {
        return (await this._read('clipboard', 'GetText', id))[0];
    }

    /** Asks notchfolkd in Shell's name, with its refusal or silence as the caller's answer. */
    async _read(name, method, ...args) {
        const {result, error, unknownMethod} = await this._daemon.attempt(name, method, ...args);
        if (error)
            throw new Error(error);
        if (!result)
            throw new Error(unknownMethod ? `notchfolkd has no ${method}` : 'notchfolkd did not answer');
        return result;
    }
}
