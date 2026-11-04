// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 zickrian

// The Shell-side proxy for the three isled methods the MCP tools need, against a stub bus and
// daemon: `gjs -m tests/extension/shellProxy.gjs.js`.

import Gio from 'gi://Gio';

const {ShellProxy} = await import('../../gnome-extension/src/shellProxy.js');

let failures = 0;
function check(what, actual, expected) {
    if (JSON.stringify(actual) === JSON.stringify(expected))
        return;
    failures++;
    printerr(`FAIL ${what}: ${JSON.stringify(actual)}, wanted ${JSON.stringify(expected)}`);
}

/** A bus that only counts what a proxy registers and unregisters. */
const bus = {
    register_object: (path, iface, handler) => {
        check('the object path', path, '/io/github/zickrian/isle/Shell');
        check('the interface name', iface.get_name(), 'io.github.zickrian.isle.Shell1');
        bus.handler = handler;
        return 7;
    },
    unregister_object: id => {
        bus.unregistered = id;
    },
};

/** isled as the test wants it: canned answers, or a refusal with its own words. */
const daemon = {
    attempt: async (name, method, ...args) => {
        const answer = daemon.answers[method];
        if (answer.error)
            return {result: null, error: answer.error, unknownMethod: false};
        return {result: answer.result, error: null, unknownMethod: false};
    },
};

const proxy = new ShellProxy(daemon, bus);
const handler = bus.handler;
check('a registered handler with the three methods',
    ['GetAppStats', 'SearchClipboard', 'GetClipboardText'].every(name => typeof handler[name] === 'function'),
    true);

daemon.answers = {GetStats: {result: ['{"today":[]}']}};
check('app stats as isled wrote them', await handler.GetAppStats(), '{"today":[]}');

daemon.answers = {Search: {result: [[3, 1]]}};
check('clipboard ids in the list order', await handler.SearchClipboard('report'), [3, 1]);

daemon.answers = {GetText: {result: ['quarterly report.txt']}};
check('a clipboard item text', await handler.GetClipboardText(3), 'quarterly report.txt');

daemon.answers = {GetText: {result: null, error: 'only GNOME Shell may call this'}};
let refused = null;
try {
    await handler.GetClipboardText(3);
} catch (e) {
    refused = e.message;
}
check("isled's refusal in the caller's answer", refused, 'only GNOME Shell may call this');

daemon.answers = {Search: {result: null, error: null}};
let silent = null;
try {
    await handler.SearchClipboard('gone');
} catch (e) {
    silent = e.message;
}
check('silence named as such', silent, 'isled did not answer');

proxy.destroy();
check('unregistered what it registered', bus.unregistered, 7);

if (failures) {
    printerr(`${failures} failed`);
    imports.system.exit(1);
}
print('shellProxy.gjs.js passed');
