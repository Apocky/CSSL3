(() => {
  'use strict';

  const body = document.querySelector('.chatroom-page');
  if (!body) return;

  const rooms = {
    north: {
      channel: 'north-clearing',
      title: 'North Clearing',
      topic: 'River recordings, field notes, and one shared question.',
      composer: 'Message #north-clearing',
      memberCount: 3,
      scope: {
        title: 'River interval',
        source: 'Mira',
        artifact: 'clip 00:41–01:12 only',
        purpose: 'name the second birdsong',
        duration: 'this session',
        x: 'Mira → selected clip → Apocrypha',
        y: 'interval → recording → North → Clearing',
        z: 'room-only · proposed audience',
        zOffered: 'selected clip offered · named audience only',
        t: 'placed → selected → previewed',
        offer: 'Sample offer active for clip 00:41–01:12, purpose: name the second birdsong, duration: this session.',
      },
      members: [
        { group: 'Here in this sample · 3', avatar: 'A', avatarClass: 'avatar-you', name: 'Apocky', state: 'sample host · local prototype' },
        { avatar: 'M', avatarClass: 'avatar-mira', name: 'Mira', state: 'sample participant · authored cast' },
        { avatar: '◈', avatarClass: 'avatar-apocrypha', name: 'Apocrypha Lab', state: 'authored response preview · not live' },
      ],
      messages: [
        { kind: 'divider', text: 'Today · sample chronology' },
        { kind: 'event', text: 'Static sample room loaded. No people, presence, messages, microphones, cameras, or services are live.' },
        {
          id: 'north-mira-river', author: 'Mira', avatar: 'M', avatarClass: 'avatar-mira', time: '11:40',
          body: 'Morning. I took the recorder down to the river bend. The water is loud, but there is a second song behind it.',
          reactions: [['≈', 3], ['◌', 2]],
        },
        {
          id: 'north-apocky-question', author: 'Apocky', avatar: 'A', avatarClass: 'avatar-you', time: '11:43',
          body: 'I hear it near the middle. I want to ask about that interval without opening the rest of the recording.',
          reactions: [['◎', 2]],
        },
        {
          id: 'north-mira-reply', author: 'Mira', avatar: 'M', avatarClass: 'avatar-mira', time: '11:46',
          body: 'That works. Keep the room quiet around it; the surrounding conversation is not part of the offer.',
        },
        {
          id: 'north-river-interval', author: 'Apocky', avatar: 'A', avatarClass: 'avatar-you', time: '12:05',
          body: 'Selected 00:41–01:12 for one question: what enters at 00:52?',
          artifact: { icon: '▶', title: 'River field recording', meta: 'clip · 00:41–01:12 · local sample' },
          crosscut: true,
          reactions: [['◇', 4], ['≈', 2]],
          thread: {
            title: 'Second birdsong',
            replies: [
              ['M', 'Mira', '12:06', 'The brighter call begins first; the descending phrase follows.'],
              ['◈', 'Authored example', 'study', 'The second song enters at 00:52—brighter, then descending.'],
              ['A', 'Apocky', '12:08', 'That resolves the timing question. Keep the interpretation marked as authored sample copy.'],
            ],
          },
        },
        { kind: 'event', tone: 'consent', text: 'The selected interval remains room-only. Apocrypha is a proposed audience; no sample offer is active.' },
        { kind: 'divider', tone: 'unread', text: 'New · authored preview' },
        {
          id: 'north-apocrypha-preview', author: 'Apocrypha Lab', avatar: '◈', avatarClass: 'avatar-apocrypha', time: 'study', badge: 'AUTHORED SAMPLE',
          body: 'The second song enters at 00:52—brighter, descending. This line is authored example copy, not a live entity response.',
          reactions: [['✦', 2]],
        },
      ],
    },
    nook: {
      channel: 'reading-nook',
      title: 'Reading Nook',
      topic: 'Letters, annotations, and quiet continuity.',
      composer: 'Message #reading-nook',
      memberCount: 3,
      scope: {
        title: 'Letter from the ridge',
        source: 'Mira',
        artifact: 'paragraph 2 only',
        purpose: 'trace the recurring river image',
        duration: 'this session',
        x: 'Mira → selected paragraph → Apocrypha',
        y: 'paragraph → letter → Nook → Clearing',
        z: 'room-only · proposed audience',
        zOffered: 'paragraph offered · named audience only',
        t: 'placed → annotated → selected',
        offer: 'Sample offer active for paragraph 2, purpose: trace the recurring river image, duration: this session.',
      },
      members: [
        { group: 'Here in this sample · 3', avatar: 'A', avatarClass: 'avatar-you', name: 'Apocky', state: 'sample host · local prototype' },
        { avatar: 'M', avatarClass: 'avatar-mira', name: 'Mira', state: 'sample correspondent · authored cast' },
        { avatar: '◈', avatarClass: 'avatar-apocrypha', name: 'Apocrypha Lab', state: 'authored response preview · not live' },
      ],
      messages: [
        { kind: 'divider', text: 'Today · sample chronology' },
        { kind: 'event', text: 'Reading Nook is a static sample channel. Nothing here is delivered, synchronized, or stored.' },
        {
          id: 'nook-mira-letter', author: 'Mira', avatar: 'M', avatarClass: 'avatar-mira', time: '11:47',
          body: 'I left the letter here rather than in a direct message. The path was silver after rain. I kept the folded map.',
          reactions: [['✦', 3]],
        },
        {
          id: 'nook-apocky-note', author: 'Apocky', avatar: 'A', avatarClass: 'avatar-you', time: '11:55',
          body: 'Paragraph two echoes the river image from North. I annotated the phrase, not the whole letter.',
        },
        {
          id: 'nook-letter-paragraph', author: 'Apocky', avatar: 'A', avatarClass: 'avatar-you', time: '12:02',
          body: 'Selected paragraph two for a single continuity question.',
          artifact: { icon: '✦', title: 'Letter from the ridge', meta: 'paragraph 2 · 4 minute read · local sample' },
          crosscut: true,
          reactions: [['§', 2], ['≈', 1]],
          thread: {
            title: 'Recurring river image',
            replies: [
              ['M', 'Mira', '12:03', 'The river is a boundary in this letter, not a destination.'],
              ['◈', 'Authored example', 'study', 'The river returns as a boundary that preserves two sides.'],
            ],
          },
        },
        { kind: 'event', tone: 'consent', text: 'Paragraph two remains room-only. No other paragraph or annotation is included.' },
        {
          id: 'nook-mira-quiet', author: 'Mira', avatar: 'M', avatarClass: 'avatar-mira', time: '12:09',
          body: 'No urgency from me. Leave the thread open until the wording feels exact.',
          reactions: [['◌', 2]],
        },
      ],
    },
    workbench: {
      channel: 'workbench',
      title: 'Workbench',
      topic: 'Notes, fragments, and unfinished things with room to breathe.',
      composer: 'Message #workbench',
      memberCount: 2,
      scope: {
        title: 'Threshold sketch 07',
        source: 'Apocky',
        artifact: 'fragment 04 only',
        purpose: 'test the boundary wording',
        duration: 'this session',
        x: 'Apocky → selected fragment → Apocrypha',
        y: 'fragment → sketch → Workbench → Clearing',
        z: 'room-only · proposed audience',
        zOffered: 'fragment offered · named audience only',
        t: 'drafted → placed → revised',
        offer: 'Sample offer active for fragment 04, purpose: test the boundary wording, duration: this session.',
      },
      members: [
        { group: 'Here in this sample · 2', avatar: 'A', avatarClass: 'avatar-you', name: 'Apocky', state: 'sample host · local prototype' },
        { avatar: '◈', avatarClass: 'avatar-apocrypha', name: 'Apocrypha Lab', state: 'authored response preview · not live' },
      ],
      messages: [
        { kind: 'divider', text: 'Earlier · sample chronology' },
        { kind: 'event', text: 'Workbench is a static sample channel for unfinished material. Silence and partial work are valid states.' },
        {
          id: 'workbench-apocky-sketch', author: 'Apocky', avatar: 'A', avatarClass: 'avatar-you', time: '09:18',
          body: 'A door can be visible without becoming an invitation. The threshold needs to show possibility without implying permission.',
          reactions: [['◇', 3]],
        },
        {
          id: 'workbench-threshold-fragment', author: 'Apocky', avatar: 'A', avatarClass: 'avatar-you', time: '09:31',
          body: 'Fragment 04 is ready for a wording test; the remaining fragments stay unfinished and room-only.',
          artifact: { icon: '◇', title: 'Threshold sketch 07', meta: 'fragment 04 · 12 fragments · local sample' },
          crosscut: true,
          reactions: [['⌁', 2], ['§', 1]],
          thread: {
            title: 'Boundary wording',
            replies: [
              ['◈', 'Authored example', 'study', 'The hinge is agency: the door remains yours to open.'],
              ['A', 'Apocky', '09:36', 'Keep hinge as the active metaphor, but remove any suggestion that visibility equals consent.'],
            ],
          },
        },
        { kind: 'event', tone: 'consent', text: 'Fragment 04 remains room-only. Visibility does not create an invitation or an active offer.' },
        {
          id: 'workbench-apocky-revision', author: 'Apocky', avatar: 'A', avatarClass: 'avatar-you', time: '09:42',
          body: 'Revision note: “The door is visible. The choice to open it remains yours.”',
          reactions: [['✓', 2]],
        },
      ],
    },
  };

  const roomButtons = [...document.querySelectorAll('[data-chat-room]')];
  const viewButtons = [...document.querySelectorAll('[data-chat-view]')];
  const inspectorTabs = [...document.querySelectorAll('[data-chat-inspector]')];
  const inspectorOpeners = [...document.querySelectorAll('[data-chat-open-inspector]')];
  const inspectorPanels = [...document.querySelectorAll('[data-chat-inspector-panel]')];
  const offerButtons = [...document.querySelectorAll('[data-chat-offer]')];
  const offerLabels = [...document.querySelectorAll('[data-chat-offer-label]')];
  const reviewOfferButtons = [...document.querySelectorAll('[data-chat-review-offer]')];
  const reviewOfferLabels = [...document.querySelectorAll('[data-chat-review-offer-label]')];
  const channelToolbar = document.querySelector('.channel-toolbar');
  const channelTitle = document.querySelector('[data-chat-channel-title]');
  const channelTopic = document.querySelector('[data-chat-channel-topic]');
  const memberCount = document.querySelector('[data-chat-member-count]');
  const messageScroller = document.querySelector('.message-scroller');
  const messageList = document.querySelector('[data-chat-messages]');
  const memberList = document.querySelector('[data-chat-members]');
  const composer = document.querySelector('[data-chat-composer]');
  const input = document.querySelector('[data-chat-input]');
  const draftStatus = document.querySelector('[data-chat-draft-status]');
  const scopeState = document.querySelector('[data-chat-scope-state]');
  const contextState = document.querySelector('[data-chat-context-state]');
  const liveStatus = document.querySelector('[data-chat-status]');
  const crosscutTitle = document.querySelector('[data-chat-crosscut-title]');
  const axisX = document.querySelector('[data-chat-axis-x]');
  const axisY = document.querySelector('[data-chat-axis-y]');
  const axisZ = document.querySelector('[data-chat-axis-z]');
  const axisT = document.querySelector('[data-chat-axis-t]');
  const scopeSource = document.querySelector('[data-chat-scope-source]');
  const scopeArtifact = document.querySelector('[data-chat-scope-artifact]');
  const scopePurpose = document.querySelector('[data-chat-scope-purpose]');
  const scopeDuration = document.querySelector('[data-chat-scope-duration]');
  const threadTitle = document.querySelector('[data-chat-thread-title]');
  const threadOrigin = document.querySelector('[data-chat-thread-origin]');
  const threadReplies = document.querySelector('[data-chat-thread-replies]');
  const resolveThreadButton = document.querySelector('[data-chat-resolve-thread]');
  const resolveThreadLabel = document.querySelector('[data-chat-resolve-label]');
  const threadCountNodes = [...document.querySelectorAll('[data-chat-open-thread-count]')];
  const threadSummary = document.querySelector('[data-chat-thread-summary]');
  const searchDialog = document.querySelector('[data-chat-search]');
  const searchOpeners = [...document.querySelectorAll('[data-chat-search-open]')];
  const searchCloser = document.querySelector('[data-chat-search-close]');
  const searchInput = document.querySelector('[data-chat-search-input]');
  const searchItems = [...document.querySelectorAll('[data-search-item]')];
  const crosscutCards = [...document.querySelectorAll('.crosscut-grid article')];
  const loomStage = document.querySelector('[data-loom-stage]');

  const localMessages = Object.fromEntries(Object.keys(rooms).map((key) => [key, []]));
  const resolvedThreads = new Set();
  let activeRoom = 'north';
  let roomInitialized = false;
  let activeThreadMessage = null;
  let offerState = 'idle';
  let offerLineage = [];
  let statusTimer = 0;
  let searchTrigger = null;
  const hubState = { activeRoom: 'north', selectedNode: null, projection: { plane: ['people', 'meaning'], depth: 'visibility', trail: 'time' }, openPanel: 'chat', draft: '' };
  function activateNode(id) { hubState.selectedNode = id || null; return roomMessageById(id); }
  function openContext(nodeId = hubState.selectedNode) { if (nodeId) activateNode(nodeId); selectInspector('crosscut', true); renderCrosscut(); }
  function previewBranch(axis, target) { crosscutCards.forEach((card) => card.classList.toggle('is-focused', card.dataset.hubContextAxis === axis)); target?.setAttribute('aria-current', 'true'); announce(`${axis[0].toUpperCase() + axis.slice(1)} context preview opened. Detail remains available by route.`); }
  function routeTo(branch) { const route = branch?.dataset?.hubRoute; if (!route) return; const params = new URLSearchParams({ from: 'clearing', node: hubState.selectedNode || 'selected', axis: branch.dataset.hubContextAxis || 'meaning' }); window.location.href = `${route}?${params.toString()}#return-to-clearing`; }
  function setProjection(projection = {}) { hubState.projection = { ...hubState.projection, ...projection }; if (loomStage) loomStage.dataset.projection = `${hubState.projection.plane.join('-')}|${hubState.projection.depth}|${hubState.projection.trail}`; return hubState.projection; }
  function returnToOrigin() { setView('chat'); messageScroller?.focus({ preventScroll: true }); announce('Returned to the conversation.'); }
  window.apockyLoom = { activateNode, openContext, previewBranch, routeTo, setProjection, returnToOrigin, state: hubState };

  function element(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function setText(node, value) {
    if (node) node.textContent = value;
  }

  function announce(message, restoreDraft = true) {
    if (!message) return;
    setText(liveStatus, message);
    if (!draftStatus) return;
    setText(draftStatus, message);
    draftStatus.title = message;
    window.clearTimeout(statusTimer);
    if (restoreDraft) {
      statusTimer = window.setTimeout(() => {
        setText(draftStatus, input?.value ? 'Local draft · never sent or stored' : 'Local draft · nothing sent or stored');
        draftStatus.removeAttribute('title');
      }, 3200);
    }
  }

  function createDivider(message) {
    const item = element('li', `chat-divider${message.tone === 'unread' ? ' is-unread' : ''}`, message.text);
    return item;
  }

  function createEvent(message) {
    const item = element('li', 'chat-event', message.text);
    if (message.tone) item.dataset.tone = message.tone;
    return item;
  }

  function createAttachment(message) {
    const attachment = element('button', 'message-attachment');
    attachment.type = 'button';
    attachment.dataset.chatAttachment = message.id;
    attachment.setAttribute('aria-label', `Preview sample attachment: ${message.artifact.title}`);
    const preview = element('span', 'attachment-preview', message.artifact.icon);
    preview.setAttribute('aria-hidden', 'true');
    const copy = element('span');
    copy.append(element('strong', '', message.artifact.title), element('small', '', message.artifact.meta));
    const action = element('span', 'attachment-action', '↗');
    action.setAttribute('aria-hidden', 'true');
    attachment.append(preview, copy, action);
    return attachment;
  }

  function createCrosscutStrip(message) {
    const strip = element('div', 'message-relation-strip');
    strip.setAttribute('aria-label', 'Open message Context');
    const button = element('button', 'relation-chip');
    button.type = 'button';
    button.dataset.chatCrosscutAxis = 'all';
    button.dataset.messageId = message.id;
    button.dataset.hubNodeId = message.id;
    button.dataset.hubNodeType = message.thread ? 'thread' : 'message';
    button.dataset.hubRoute = 'clearing.html';
    button.setAttribute('aria-label', 'Open four-path Context for this message');
    button.append(element('b', '', '◇'), element('span', '', 'Context'));
    strip.append(button);
    return strip;
  }

  function createMessageActions(message) {
    const actions = element('div', 'message-actions');
    if (message.reactions) {
      message.reactions.forEach(([glyph, count], index) => {
        const button = element('button', 'reaction-button', `${glyph} ${count}`);
        button.type = 'button';
        button.dataset.chatReaction = String(index);
        button.dataset.baseCount = String(count);
        button.setAttribute('aria-pressed', 'false');
        button.setAttribute('aria-label', `Add local sample reaction ${glyph}; ${count} sample reactions shown`);
        actions.append(button);
      });
    }
    if (message.thread) {
      const key = `${activeRoom}:${message.id}`;
      const resolved = resolvedThreads.has(key);
      const button = element('button', 'thread-button', `${resolved ? '✓ Resolved' : `${message.thread.replies.length} replies`}`);
      button.type = 'button';
      button.dataset.chatThreadId = message.id;
      actions.append(button);
    }
    if (message.crosscut) {
      const button = element('button', 'message-context-button', 'Context');
      button.type = 'button';
      button.dataset.chatCrosscutAxis = 'all';
      button.dataset.messageId = message.id;
      button.dataset.hubNodeId = message.id;
      button.dataset.hubNodeType = message.thread ? 'thread' : 'message';
      button.dataset.hubRoute = 'clearing.html';
      button.setAttribute('aria-label', 'Open four-path Context for this message');
      actions.append(button);
    }
    return actions;
  }

  function createChatMessage(message) {
    const item = element('li', `chat-message${message.local ? ' is-local' : ''}`);
    item.dataset.messageId = message.id;
    item.dataset.hubNodeId = message.id;
    item.dataset.hubNodeType = message.thread ? 'thread' : 'message';
    const avatar = element('span', `chat-avatar ${message.avatarClass || ''}`, message.avatar);
    avatar.setAttribute('aria-hidden', 'true');
    const main = element('article', 'chat-message-main');
    const header = element('header', 'chat-message-header');
    header.append(element('strong', '', message.author));
    if (message.badge) header.append(element('span', 'author-badge', message.badge));
    const time = element('time', '', message.time);
    header.append(time);
    main.append(header, element('p', 'chat-message-body', message.body));
    if (message.artifact) main.append(createAttachment(message));
    if (message.crosscut) main.append(createCrosscutStrip(message));
    const actions = createMessageActions(message);
    if (actions.childElementCount) main.append(actions);
    item.append(avatar, main);
    return item;
  }

  function renderMessages() {
    if (!messageList) return;
    const content = rooms[activeRoom];
    const messages = [...content.messages, ...localMessages[activeRoom]];
    messageList.replaceChildren(...messages.map((message) => {
      if (message.kind === 'divider') return createDivider(message);
      if (message.kind === 'event') return createEvent(message);
      return createChatMessage(message);
    }));
  }

  function renderMembers() {
    if (!memberList) return;
    const nodes = [];
    rooms[activeRoom].members.forEach((member) => {
      if (member.group) nodes.push(element('li', 'member-group-title', member.group));
      const item = element('li', 'member-item');
      const avatar = element('span', `member-avatar ${member.avatarClass || ''}`, member.avatar);
      avatar.setAttribute('aria-hidden', 'true');
      const copy = element('span');
      copy.append(element('strong', '', member.name), element('small', '', member.state));
      item.append(avatar, copy);
      nodes.push(item);
    });
    memberList.replaceChildren(...nodes);
  }

  function roomMessageById(id) {
    return rooms[activeRoom].messages.find((message) => message.id === id) || localMessages[activeRoom].find((message) => message.id === id);
  }

  function firstThreadMessage() {
    return rooms[activeRoom].messages.find((message) => message.thread) || null;
  }

  function updateThreadSummary() {
    const threads = rooms[activeRoom].messages.filter((message) => message.thread);
    const open = threads.filter((message) => !resolvedThreads.has(`${activeRoom}:${message.id}`)).length;
    threadCountNodes.forEach((node) => setText(node, String(open)));
    setText(threadSummary, open === 1 ? '1 open thread' : `${open} open threads`);
  }

  function renderThread(message = firstThreadMessage()) {
    activeThreadMessage = message;
    if (!message?.thread) {
      setText(threadTitle, 'No sample thread');
      setText(threadOrigin, 'This room has no authored sample thread.');
      threadReplies?.replaceChildren();
      if (resolveThreadButton) resolveThreadButton.disabled = true;
      return;
    }
    if (resolveThreadButton) resolveThreadButton.disabled = false;
    setText(threadTitle, message.thread.title);
    setText(threadOrigin, `${message.author}: “${message.body}”`);
    if (threadReplies) {
      threadReplies.replaceChildren(...message.thread.replies.map(([avatarText, author, timeText, reply]) => {
        const item = element('li', 'thread-reply');
        const avatar = element('span', 'mini-avatar', avatarText);
        avatar.setAttribute('aria-hidden', 'true');
        const copy = element('article');
        const header = element('header');
        header.append(element('strong', '', author), element('time', '', timeText));
        copy.append(header, element('p', '', reply));
        item.append(avatar, copy);
        return item;
      }));
    }
    const resolved = resolvedThreads.has(`${activeRoom}:${message.id}`);
    resolveThreadButton?.setAttribute('aria-pressed', String(resolved));
    setText(resolveThreadLabel, resolved ? 'Reopen sample thread' : 'Mark sample thread resolved');
  }

  function renderCrosscut(axis = 'all') {
    const scope = rooms[activeRoom].scope;
    setText(crosscutTitle, scope.title);
    setText(axisX, scope.x);
    setText(axisY, scope.y);
    setText(axisZ, offerState === 'offered' ? scope.zOffered : scope.z);
    setText(axisT, [scope.t, ...offerLineage].join(' → '));
    setText(scopeSource, scope.source);
    setText(scopeArtifact, scope.artifact);
    setText(scopePurpose, scope.purpose);
    setText(scopeDuration, scope.duration);
    const axes = ['x', 'y', 'z', 't'];
    crosscutCards.forEach((card, index) => card.classList.toggle('is-focused', axis === axes[index]));
  }

  function setView(view) {
    if (!['channels', 'chat', 'people'].includes(view)) return;
    body.dataset.chatViewMode = view;
    viewButtons.forEach((button) => button.setAttribute('aria-pressed', String(button.dataset.chatView === view)));
  }

  function selectInspector(mode, moveMobile = false) {
    if (!['members', 'thread', 'crosscut'].includes(mode)) return;
    body.dataset.chatInspectorMode = mode;
    inspectorTabs.forEach((button) => button.setAttribute('aria-selected', String(button.dataset.chatInspector === mode)));
    inspectorPanels.forEach((panel) => {
      panel.hidden = panel.dataset.chatInspectorPanel !== mode;
    });
    if (mode === 'thread') renderThread(activeThreadMessage || firstThreadMessage());
    if (mode === 'crosscut') renderCrosscut();
    if (moveMobile && window.matchMedia('(max-width: 980px)').matches) {
      setView('people');
      const selectedTab = inspectorTabs.find((button) => button.dataset.chatInspector === mode);
      window.queueMicrotask(() => selectedTab?.focus({ preventScroll: true }));
    }
  }

  function syncOfferState(next, message) {
    offerState = next;
    if (next === 'idle') offerLineage = [];
    else if (offerLineage.at(-1) !== next) offerLineage.push(next);
    body.dataset.chatOfferState = next;
    const offered = next === 'offered';
    const withdrawn = next === 'withdrawn';
    const label = offered ? 'Withdraw sample' : 'Offer sample';
    offerLabels.forEach((node) => setText(node, label));
    reviewOfferLabels.forEach((node) => setText(node, offered ? 'Review / withdraw' : 'Review offer'));
    setText(scopeState, offered ? '1 sample offer' : withdrawn ? 'Offer withdrawn' : 'Room only');
    setText(contextState, offered ? 'Sample offer active' : withdrawn ? 'Sample offer withdrawn' : 'No sample offer');
    renderCrosscut();
    if (message) announce(message);
  }

  function resetInput() {
    if (!input) return;
    input.value = '';
  }

  function showChatAfterRoomSelection() {
    if (!window.matchMedia('(max-width: 980px)').matches) return;
    setView('chat');
    if (searchDialog?.open) return;
    window.queueMicrotask(() => channelToolbar?.focus({ preventScroll: true }));
  }

  function activateRoom(key, { announceChange = true, updateHash = true } = {}) {
    const content = rooms[key];
    if (!content) return;
    const changed = key !== activeRoom;
    if (roomInitialized && !changed) {
      showChatAfterRoomSelection();
      if (announceChange) announce(`${content.title} sample is already selected. Draft and offer state were preserved.`);
      return;
    }
    const wasInitialized = roomInitialized;
    roomInitialized = true;
    activeRoom = key;
    roomButtons.forEach((button) => {
      const selected = button.dataset.chatRoom === key;
      button.classList.toggle('is-active', selected);
      button.setAttribute('aria-pressed', String(selected));
    });
    setText(channelTitle, content.channel);
    setText(channelTopic, content.topic);
    setText(memberCount, String(content.memberCount));
    if (input) input.placeholder = content.composer;
    if (messageScroller) messageScroller.setAttribute('aria-label', `Sample conversation in ${content.channel}`);
    resetInput();
    syncOfferState('idle');
    renderMessages();
    renderMembers();
    renderCrosscut();
    activeThreadMessage = firstThreadMessage();
    renderThread(activeThreadMessage);
    updateThreadSummary();
    if (updateHash && window.location.hash !== `#${key}`) window.history.replaceState(null, '', `#${key}`);
    window.queueMicrotask(() => {
      if (messageScroller) messageScroller.scrollTop = messageScroller.scrollHeight;
    });
    if (wasInitialized && changed) showChatAfterRoomSelection();
    if (announceChange) {
      announce(changed
        ? `${content.title} selected. Local draft and sample offer did not carry across rooms.`
        : `${content.title} sample is already selected.`);
    } else {
      setText(liveStatus, `${content.title} static sample loaded. Nothing is connected or sent.`);
    }
  }

  roomButtons.forEach((button, index) => {
    button.addEventListener('click', () => activateRoom(button.dataset.chatRoom));
    button.addEventListener('keydown', (event) => {
      let next = null;
      if (event.key === 'ArrowDown' || event.key === 'ArrowRight') next = (index + 1) % roomButtons.length;
      if (event.key === 'ArrowUp' || event.key === 'ArrowLeft') next = (index - 1 + roomButtons.length) % roomButtons.length;
      if (event.key === 'Home') next = 0;
      if (event.key === 'End') next = roomButtons.length - 1;
      if (next === null) return;
      event.preventDefault();
      roomButtons[next].focus();
      activateRoom(roomButtons[next].dataset.chatRoom);
    });
  });

  viewButtons.forEach((button) => button.addEventListener('click', () => setView(button.dataset.chatView)));

  inspectorTabs.forEach((button, index) => {
    button.addEventListener('click', () => selectInspector(button.dataset.chatInspector));
    button.addEventListener('keydown', (event) => {
      let next = null;
      if (event.key === 'ArrowRight' || event.key === 'ArrowDown') next = (index + 1) % inspectorTabs.length;
      if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') next = (index - 1 + inspectorTabs.length) % inspectorTabs.length;
      if (event.key === 'Home') next = 0;
      if (event.key === 'End') next = inspectorTabs.length - 1;
      if (next === null) return;
      event.preventDefault();
      inspectorTabs[next].focus();
      selectInspector(inspectorTabs[next].dataset.chatInspector);
    });
  });

  inspectorOpeners.forEach((button) => button.addEventListener('click', () => selectInspector(button.dataset.chatOpenInspector, true)));

  offerButtons.forEach((button) => {
    button.addEventListener('click', () => {
      if (offerState === 'offered') {
        syncOfferState('withdrawn', 'Sample offer withdrawn. The selected artifact is room-only again; a local receipt remains visible only in this page session.');
      } else {
        syncOfferState('offered', rooms[activeRoom].scope.offer);
      }
    });
  });

  reviewOfferButtons.forEach((button) => {
    button.addEventListener('click', () => {
      selectInspector('crosscut', true);
      announce(offerState === 'offered'
        ? 'Review the exact active sample offer before choosing whether to withdraw it.'
        : 'Review the exact artifact, audience, purpose, and duration before offering this sample.');
    });
  });

  messageList?.addEventListener('click', (event) => {
    const target = event.target.closest('button');
    if (!target) return;
    if (target.matches('[data-chat-reaction]')) {
      const pressed = target.getAttribute('aria-pressed') === 'true';
      const base = Number(target.dataset.baseCount || 0);
      const glyph = target.textContent.trim().split(' ')[0];
      const nextPressed = !pressed;
      const nextCount = base + (pressed ? 0 : 1);
      target.setAttribute('aria-pressed', String(nextPressed));
      target.setAttribute('aria-label', `${nextPressed ? 'Remove' : 'Add'} local sample reaction ${glyph}; ${nextCount} sample reactions shown`);
      target.textContent = `${glyph} ${nextCount}`;
      announce(pressed ? 'Local sample reaction removed.' : 'Local sample reaction added. Nothing was sent or stored.');
      return;
    }
    if (target.matches('[data-chat-thread-id]')) {
      const message = roomMessageById(target.dataset.chatThreadId);
      if (!message?.thread) return;
      renderThread(message);
      selectInspector('thread', true);
      announce(`Opened the local sample thread: ${message.thread.title}.`);
      return;
    }
    if (target.matches('[data-chat-crosscut-axis]')) {
      const axis = target.dataset.chatCrosscutAxis;
      activateNode(target.dataset.hubNodeId || target.dataset.messageId);
      selectInspector('crosscut', true);
      renderCrosscut(axis);
      if (axis !== 'all') crosscutCards[['x', 'y', 'z', 't'].indexOf(axis)]?.scrollIntoView({ block: 'nearest' });
      announce(axis === 'all' ? 'Opened the message Context.' : `Opened ${axis.toUpperCase()} message context.`);
      return;
    }
    if (target.matches('[data-chat-attachment]')) {
      announce('Attachment preview is visual only. No file was opened, uploaded, or transmitted.');
    }
  });

  crosscutCards.forEach((card) => {
    const activate = () => previewBranch(card.dataset.hubContextAxis, card);
    card.addEventListener('click', (event) => { if (event.target.closest('a,button')) return; activate(); });
    card.addEventListener('keydown', (event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); activate(); } });
    card.addEventListener('dblclick', () => routeTo(card));
    card.querySelector('[data-hub-route-link]')?.addEventListener('click', (event) => { event.preventDefault(); routeTo(card); });
  });

  resolveThreadButton?.addEventListener('click', () => {
    if (!activeThreadMessage?.thread) return;
    const key = `${activeRoom}:${activeThreadMessage.id}`;
    const resolved = resolvedThreads.has(key);
    if (resolved) resolvedThreads.delete(key);
    else resolvedThreads.add(key);
    renderThread(activeThreadMessage);
    renderMessages();
    updateThreadSummary();
    announce(resolved
      ? 'Sample thread reopened locally.'
      : 'Sample thread marked resolved locally. This records meaningful closure, not a score or streak.');
  });

  composer?.addEventListener('submit', (event) => {
    event.preventDefault();
    const message = input?.value.trim();
    if (!message) {
      announce('Write a local message first. Nothing is sent from this prototype.');
      input?.focus();
      return;
    }
    const id = `${activeRoom}-local-${Date.now()}`;
    localMessages[activeRoom].push({
      id,
      author: 'You',
      avatar: 'A',
      avatarClass: 'avatar-you',
      time: 'now',
      badge: 'LOCAL PREVIEW',
      body: message,
      local: true,
      reactions: [],
    });
    resetInput();
    renderMessages();
    if (messageScroller) messageScroller.scrollTop = messageScroller.scrollHeight;
    announce('Local preview message placed in this room. Nothing was sent, stored, or shared.');
  });

  input?.addEventListener('input', () => {
    setText(draftStatus, input.value ? 'Local draft · never sent or stored' : 'Local draft · nothing sent or stored');
  });

  document.querySelectorAll('[data-chat-demo-action]').forEach((button) => {
    button.addEventListener('click', () => announce(button.dataset.chatDemoAction));
  });

  function filterSearch() {
    const query = searchInput?.value.trim().toLowerCase() || '';
    searchItems.forEach((item) => {
      item.hidden = Boolean(query) && !item.textContent.toLowerCase().includes(query);
    });
  }

  function openSearch(trigger) {
    if (!searchDialog) return;
    searchTrigger = trigger || document.activeElement;
    if (typeof searchDialog.showModal === 'function') searchDialog.showModal();
    else searchDialog.setAttribute('open', '');
    if (searchInput) {
      searchInput.value = '';
      filterSearch();
      searchInput.focus();
    }
  }

  function closeSearch() {
    if (!searchDialog) return;
    if (typeof searchDialog.close === 'function') searchDialog.close();
    else searchDialog.removeAttribute('open');
  }

  searchOpeners.forEach((button) => button.addEventListener('click', () => openSearch(button)));
  searchCloser?.addEventListener('click', closeSearch);
  searchInput?.addEventListener('input', filterSearch);
  searchInput?.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    closeSearch();
  });
  searchDialog?.addEventListener('close', () => searchTrigger?.focus());
  searchDialog?.addEventListener('click', (event) => {
    if (event.target === searchDialog) closeSearch();
  });
  searchItems.forEach((item) => {
    if (!item.matches('[data-search-room]')) return;
    item.addEventListener('click', () => {
      activateRoom(item.dataset.searchRoom);
      closeSearch();
      setView('chat');
    });
  });

  document.addEventListener('keydown', (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      if (searchDialog?.open) closeSearch();
      else openSearch(document.activeElement);
    }
  });

  window.addEventListener('hashchange', () => {
    const requested = window.location.hash.slice(1);
    if (rooms[requested] && requested !== activeRoom) activateRoom(requested, { updateHash: false });
  });

  const requestedRoom = window.location.hash.slice(1);
  activateRoom(rooms[requestedRoom] ? requestedRoom : 'north', { announceChange: false, updateHash: false });
  selectInspector('members');
  setView('chat');
})();
