// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 zickrian

// The agent ask's card rules, which need no GNOME: `node --test tests/extension/*.test.js`.

import assert from 'node:assert/strict';
import {test} from 'node:test';

import {agentAskCard, isAgentAskCard} from '../../gnome-extension/src/actionRules.js';

test('an agent ask becomes a waiting card named by its ask', () => {
    const card = agentAskCard({id: 'omp-3', tool: 'bash', input: 'firefox\nother line'});
    assert.equal(card.id, 'ask:omp-3');
    assert.equal(card.why, 'bash');
    // The card shows the first line of the input; the whole of it is the command.
    assert.equal(card.detail, 'firefox');
    assert.equal(card.command, 'firefox\nother line');
    assert.equal(card.status, 'waiting');
    assert.equal(card.risks, null);
});

test('an ask with no input still says its tool', () => {
    const card = agentAskCard({id: 'hermes-1', tool: 'write_file'});
    assert.equal(card.command, 'write_file');
    assert.equal(card.detail, '');
    // Input that is not a string at all reads as none.
    const odd = agentAskCard({id: 'x', tool: 'bash', input: {command: 'ls'}});
    assert.equal(odd.command, 'bash');
    assert.equal(odd.detail, '');
});

test('a card is an ask by its id, and nothing else is', () => {
    assert.equal(isAgentAskCard(agentAskCard({id: 'omp-1', tool: 'bash'})), true);
    assert.equal(isAgentAskCard({id: 'run:2'}), false);
    assert.equal(isAgentAskCard(null), false);
    assert.equal(isAgentAskCard({}), false);
});
