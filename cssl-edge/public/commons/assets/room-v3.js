(() => {
  'use strict';

  const body = document.body;
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const saveData = Boolean(navigator.connection?.saveData);
  const roomIndex = document.querySelector('[data-room-index]');
  const roomChoices = [...document.querySelectorAll('[data-room]')];
  const roomTitle = document.querySelector('[data-room-title]');
  const roomTopic = document.querySelector('[data-room-topic]');
  const roomOpeners = [...document.querySelectorAll('[data-room-index-open]')];
  const roomCloser = document.querySelector('[data-room-index-close]');
  const contextSheet = document.querySelector('[data-context-sheet]');
  const contextOpeners = [...document.querySelectorAll('[data-context-open]')];
  const contextCloser = document.querySelector('[data-context-close]');
  const contextReturn = document.querySelector('[data-context-return]');
  const contextBranches = [...document.querySelectorAll('[data-axis]')];
  const contextGlyph = document.querySelector('[data-context-glyph]');
  const contextLabel = document.querySelector('[data-context-label]');
  const contextTitle = document.querySelector('[data-context-title]');
  const contextCopy = document.querySelector('[data-context-copy]');
  const contextRoute = document.querySelector('[data-context-route]');
  const focusStage = document.querySelector('[data-focus-stage]');
  const focusOpeners = [...document.querySelectorAll('[data-focus-open]')];
  const focusCloser = document.querySelector('[data-focus-close]');
  const composer = document.querySelector('[data-composer]');
  const messageInput = document.querySelector('[data-message-input]');
  const messageList = document.querySelector('[data-messages]');
  const messageStream = document.querySelector('[data-message-stream]');
  const motionToggle = document.querySelector('[data-motion-toggle]');
  const motionLabel = document.querySelector('[data-motion-label]');
  const liveStatus = document.querySelector('[data-live-status]');
  let lastContextTrigger = null;
  let lastFocusTrigger = null;
  let statusTimer = 0;
  let glossFrame = 0;
  let glossTarget = null;
  let glossX = 0;
  let glossY = 0;

  const rooms = {
    north: {
      title: 'north-clearing',
      topic: 'River recordings, field notes, and one shared question.',
      status: 'North Clearing sample selected.',
    },
    nook: {
      title: 'reading-nook',
      topic: 'Letters, annotations, and quiet continuity.',
      status: 'Reading Nook sample selected. Conversation content remains an authored layout study.',
    },
    workbench: {
      title: 'workbench',
      topic: 'Notes, fragments, and unfinished things with room to breathe.',
      status: 'Workbench sample selected. Conversation content remains an authored layout study.',
    },
  };

  const axes = {
    people: {
      glyph: '◎',
      label: 'PEOPLE',
      title: 'Three authored roles',
      copy: 'Mira → selected clip → Apocky → invited participant study.',
      route: '/membership?from=clearing&origin=river-interval&axis=people',
      routeLabel: 'Open people detail',
      opensNewTab: true,
    },
    meaning: {
      glyph: '⌗',
      label: 'MEANING',
      title: 'Inside North Clearing',
      copy: 'Interval → recording → room → community study.',
      route: '/atlas?from=clearing&origin=river-interval&axis=meaning',
      routeLabel: 'Open related detail',
      opensNewTab: true,
    },
    visibility: {
      glyph: '◌',
      label: 'VISIBILITY',
      title: 'Room-only sample',
      copy: 'The selected clip is represented locally; no offer or transmission is active.',
      route: '/principles?from=clearing&origin=river-interval&axis=visibility',
      routeLabel: 'Open visibility rules',
      opensNewTab: true,
    },
    time: {
      glyph: '↝',
      label: 'TIME',
      title: 'Placed → selected → previewed',
      copy: 'A static lineage trail preserves how the selected interval arrived here.',
      route: '#conversation',
      routeLabel: 'Return to chronology',
      opensNewTab: false,
    },
  };

  function announce(message) {
    if (!liveStatus) return;
    window.clearTimeout(statusTimer);
    liveStatus.textContent = message;
    liveStatus.classList.add('is-visible');
    statusTimer = window.setTimeout(() => liveStatus.classList.remove('is-visible'), 2600);
  }

  function setRoomIndex(open) {
    roomIndex?.classList.toggle('is-open', open);
    roomIndex?.setAttribute('aria-hidden', String(!open && window.matchMedia('(max-width: 1080px)').matches));
    if (open) window.queueMicrotask(() => roomChoices.find((choice) => choice.getAttribute('aria-pressed') === 'true')?.focus());
  }

  function setContext(open, trigger = null) {
    if (!contextSheet) return;
    if (open && trigger) lastContextTrigger = trigger;
    contextSheet.classList.toggle('is-open', open);
    contextSheet.setAttribute('aria-hidden', String(!open));
    contextOpeners.forEach((opener) => opener.setAttribute('aria-expanded', String(open)));
    if (open) {
      setRoomIndex(false);
      window.queueMicrotask(() => contextCloser?.focus());
      announce('Selected message Context opened.');
    } else {
      lastContextTrigger?.focus({ preventScroll: true });
      announce('Returned to the selected message.');
    }
  }

  function setAxis(axis) {
    const next = axes[axis];
    if (!next) return;
    contextBranches.forEach((branch) => branch.setAttribute('aria-pressed', String(branch.dataset.axis === axis)));
    if (contextGlyph) contextGlyph.textContent = next.glyph;
    if (contextLabel) contextLabel.textContent = next.label;
    if (contextTitle) contextTitle.textContent = next.title;
    if (contextCopy) contextCopy.textContent = next.copy;
    if (contextRoute) {
      contextRoute.href = next.route;
      contextRoute.childNodes[0].textContent = `${next.routeLabel} `;
      contextRoute.toggleAttribute('target', next.opensNewTab);
      if (next.opensNewTab) {
        contextRoute.target = '_blank';
        contextRoute.rel = 'noopener';
      } else {
        contextRoute.removeAttribute('rel');
      }
      contextRoute.setAttribute('aria-label', `${next.routeLabel}${next.opensNewTab ? ', opens in a new tab' : ''}`);
    }
    announce(`${next.label.toLowerCase()} Context preview selected.`);
  }

  function setMotionOff(off) {
    body.classList.toggle('motion-off', off);
    motionToggle?.setAttribute('aria-pressed', String(off));
    if (motionLabel) motionLabel.textContent = off ? 'Motion' : 'Still';
    motionToggle?.setAttribute('aria-label', off ? 'Enable optional motion' : 'Reduce optional motion');
    announce(off ? 'Optional motion reduced.' : 'Optional motion enabled.');
  }

  function addLocalMessage(text) {
    if (!messageList) return;
    const item = document.createElement('li');
    item.className = 'message-cluster';
    item.dataset.nodeId = `local-${Date.now()}`;
    item.dataset.nodeType = 'message';

    const avatar = document.createElement('span');
    avatar.className = 'avatar avatar--gold';
    avatar.setAttribute('aria-hidden', 'true');
    avatar.textContent = 'A';

    const article = document.createElement('article');
    const header = document.createElement('header');
    const author = document.createElement('strong');
    const time = document.createElement('time');
    const marker = document.createElement('span');
    author.textContent = 'You';
    time.textContent = 'now';
    marker.textContent = 'SAMPLE DRAFT';
    header.append(author, time, marker);

    const copy = document.createElement('p');
    copy.textContent = text;
    const actions = document.createElement('div');
    actions.className = 'message-actions';
    actions.setAttribute('aria-label', 'Local message actions');
    const reply = document.createElement('button');
    reply.type = 'button';
    reply.dataset.reply = '';
    reply.textContent = 'Reply';
    actions.append(reply);
    article.append(header, copy, actions);
    item.append(avatar, article);
    messageList.append(item);
    item.scrollIntoView({ block: 'nearest', behavior: body.classList.contains('motion-off') || reducedMotion.matches || saveData ? 'auto' : 'smooth' });
  }

  function updateGloss() {
    glossFrame = 0;
    if (!glossTarget?.isConnected) return;
    const rect = glossTarget.getBoundingClientRect();
    if (!rect.width || !rect.height) return;
    glossTarget.style.setProperty('--mx', `${Math.max(0, Math.min(100, ((glossX - rect.left) / rect.width) * 100))}%`);
    glossTarget.style.setProperty('--my', `${Math.max(0, Math.min(100, ((glossY - rect.top) / rect.height) * 100))}%`);
  }

  document.addEventListener('pointermove', (event) => {
    if (body.classList.contains('motion-off') || reducedMotion.matches || saveData) return;
    const target = event.target.closest('.material');
    if (!target) return;
    glossTarget = target;
    glossX = event.clientX;
    glossY = event.clientY;
    if (!glossFrame) glossFrame = window.requestAnimationFrame(updateGloss);
  }, { passive: true });

  roomOpeners.forEach((opener) => opener.addEventListener('click', () => setRoomIndex(true)));
  roomCloser?.addEventListener('click', () => setRoomIndex(false));
  document.querySelector('[data-mobile-chat]')?.addEventListener('click', () => {
    setRoomIndex(false);
    setContext(false);
    messageStream?.focus({ preventScroll: true });
  });

  roomChoices.forEach((choice) => {
    choice.addEventListener('click', () => {
      const room = rooms[choice.dataset.room];
      if (!room) return;
      roomChoices.forEach((item) => {
        const active = item === choice;
        item.classList.toggle('is-active', active);
        item.setAttribute('aria-pressed', String(active));
      });
      if (roomTitle) roomTitle.textContent = room.title;
      if (roomTopic) roomTopic.textContent = room.topic;
      document.documentElement.dataset.activeRoom = choice.dataset.room;
      setRoomIndex(false);
      announce(room.status);
    });
  });

  contextOpeners.forEach((opener) => opener.addEventListener('click', () => setContext(true, opener)));
  contextCloser?.addEventListener('click', () => setContext(false));
  contextReturn?.addEventListener('click', () => setContext(false));
  contextBranches.forEach((branch) => branch.addEventListener('click', () => setAxis(branch.dataset.axis)));

  focusOpeners.forEach((opener) => {
    opener.addEventListener('click', () => {
      lastFocusTrigger = opener;
      if (typeof focusStage?.showModal === 'function') focusStage.showModal();
      focusCloser?.focus();
      announce('Local artifact focus stage opened.');
    });
  });
  focusCloser?.addEventListener('click', () => focusStage?.close());
  focusStage?.addEventListener('click', (event) => {
    if (event.target === focusStage) focusStage.close();
  });
  focusStage?.addEventListener('close', () => {
    lastFocusTrigger?.focus({ preventScroll: true });
    announce('Artifact focus stage closed.');
  });

  messageList?.addEventListener('click', (event) => {
    const reaction = event.target.closest('[data-reaction]');
    if (reaction) {
      const pressed = reaction.getAttribute('aria-pressed') === 'true';
      const count = reaction.querySelector('span');
      const base = Number(count?.textContent || 0);
      reaction.setAttribute('aria-pressed', String(!pressed));
      if (count) count.textContent = String(base + (pressed ? -1 : 1));
      announce(pressed ? 'Local sample reaction removed.' : 'Local sample reaction added. Nothing was sent.');
      return;
    }
    if (event.target.closest('[data-reply]')) {
      messageInput?.focus();
      announce('Composer focused for a local sample reply.');
      return;
    }
    if (event.target.closest('[data-thread]')) announce('Three authored sample replies are represented in the original room study.');
  });

  composer?.addEventListener('submit', (event) => {
    event.preventDefault();
    const text = messageInput?.value.trim();
    if (!text) {
      messageInput?.focus();
      announce('Write a local sample message first.');
      return;
    }
    addLocalMessage(text);
    messageInput.value = '';
    announce('Local preview message added. Nothing was sent or stored.');
  });

  motionToggle?.addEventListener('click', () => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setMotionOff(true);
      announce('System reduced-motion preference remains authoritative.');
      return;
    }
    setMotionOff(!body.classList.contains('motion-off'));
  });

  document.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape') return;
    if (focusStage?.open) {
      focusStage.close();
      return;
    }
    if (contextSheet?.classList.contains('is-open')) {
      setContext(false);
      return;
    }
    setRoomIndex(false);
  });

  setRoomIndex(false);
  if (reducedMotion.matches || saveData) setMotionOff(true);
  reducedMotion.addEventListener?.('change', (event) => setMotionOff(event.matches));
})();
