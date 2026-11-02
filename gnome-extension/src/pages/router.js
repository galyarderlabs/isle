// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 zickrian

// Router: the LLM gateway's own usage — requests, tokens and the cost it estimates — for the
// period Settings picked: a summary card, the busiest models as bars, and the last requests.

import Clutter from 'gi://Clutter';
import GObject from 'gi://GObject';
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

        const layout = new St.BoxLayout({style_class: 'isle-router-layout', x_expand: true, y_expand: true});
        const summary = new St.BoxLayout({...VERTICAL, style_class: 'isle-router-summary'});

        const heading = new St.BoxLayout({style_class: 'isle-router-heading'});
        heading.add_child(new St.Icon({gicon: icon('network-router-symbolic'), icon_size: 16}));
        heading.add_child(label('Router', 'isle-router-title'));
        // The period toggle in the header, kept in step with Settings.
        this._periodToggle = new St.BoxLayout({style_class: 'isle-router-periods'});
        this._periodButtons = {};
        for (const period of PERIODS) {
            const button = new St.Button({style_class: 'isle-router-period', can_focus: true,
                label: PERIOD_LABELS[period], accessible_name: `Router period ${PERIOD_LABELS[period]}`});
            button.connect('clicked', () => this._setPeriod(period));
            this._periodToggle.add_child(button);
            this._periodButtons[period] = button;
        }
        heading.add_child(this._periodToggle);
        summary.add_child(heading);

        // The summary card: the period's totals. The gateway's cost is its own estimate — the
        // "est." never leaves it.
        this._summaryLine = label('', 'isle-router-totals');
        summary.add_child(this._summaryLine);
        this._statusLine = label('', 'isle-router-status');
        summary.add_child(this._statusLine);
        layout.add_child(summary);

        // Everything past the summary in one column the wheel scrolls when it outgrows the
        // island, like the Tasks list (no scrollbar over the content; a fade at its edge).
        const column = new St.BoxLayout({...VERTICAL, style_class: 'isle-router-content', x_expand: true,
            y_expand: true});
        this._models = new St.BoxLayout({...VERTICAL, style_class: 'isle-router-models', x_expand: true});
        column.add_child(this._models);
        this._recent = new St.BoxLayout({...VERTICAL, style_class: 'isle-router-recent', x_expand: true});
        column.add_child(this._recent);
        this._scroll = new St.ScrollView({
            style_class: 'isle-router-scroll',
            hscrollbar_policy: St.PolicyType.NEVER,
            // Scrolls with the wheel or touchpad; no scrollbar over the list.
            vscrollbar_policy: St.PolicyType.EXTERNAL,
            x_expand: true,
            y_expand: true,
        });
        this._scroll.set_child(column);
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
        const cost = `$${(stats.cost ?? 0).toFixed(2)} est`;
        this._summaryLine.text = router?.status === 'connected'
            ? `${stats.requests ?? 0} requests · ${tokens(stats.prompt_tokens)} in · ${tokens(stats.completion_tokens)} out · ${cost}`
            : '';
        this._statusLine.text = {
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
        this._models.destroy_all_children();
        const top = byModel.slice(0, TOP_MODELS);
        const widest = Math.max(1, ...top.map(usage => usage.tokens));
        for (const usage of top) {
            const row = new St.BoxLayout({style_class: 'isle-router-model'});
            const name = label(usage.model, 'isle-router-model-name');
            row.add_child(name);
            const track = new St.Widget({style_class: 'isle-router-model-track', x_expand: true});
            const fill = new St.Widget({style_class: 'isle-router-model-fill'});
            fill.width = Math.max(px(2), Math.round(px(120) * usage.tokens / widest));
            track.add_child(fill);
            row.add_child(track);
            row.add_child(label(tokens(usage.tokens), 'isle-router-model-tokens'));
            this._models.add_child(row);
        }
    }

    /** Rebuilds the last requests: model, in→out tokens, ok/error, relative time. */
    _buildRecent(recent) {
        this._recent.destroy_all_children();
        for (const request of recent) {
            const row = new St.BoxLayout({style_class: 'isle-router-request'});
            row.add_child(label(request.model, 'isle-router-request-model'));
            row.add_child(label(`${tokens(request.prompt_tokens)} → ${tokens(request.completion_tokens)}`,
                'isle-router-request-tokens'));
            row.add_child(label(request.ok ? 'ok' : 'error',
                `isle-router-request-ok${request.ok ? '' : ' error'}`));
            row.add_child(label(request.at_ms ? ago(request.at_ms) : '', 'isle-router-request-time'));
            this._recent.add_child(row);
        }
    }
});
