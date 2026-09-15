(() => {
  'use strict';

  const root = document.documentElement;
  const body = document.body;
  const searchParams = new URLSearchParams(window.location.search);
  const hashParams = new URLSearchParams(window.location.hash.replace(/^#/, ''));
  const hasAuthCallback = searchParams.has('code')
    || searchParams.has('error')
    || hashParams.has('access_token')
    || hashParams.has('error');
  if (window.location.pathname === '/' && hasAuthCallback) {
    window.location.replace(`/auth/callback${window.location.search}${window.location.hash}`);
    return;
  }
  const reduceQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
  const finePointerQuery = window.matchMedia('(hover: hover) and (pointer: fine)');
  const connection = navigator.connection || navigator.mozConnection || navigator.webkitConnection;
  const storageKey = 'apocky-prototype-stillness';
  const persistMotionChoice = !body.classList.contains('chatroom-page');
  let userMotionChoice = null;

  root.classList.add('js');

  if (persistMotionChoice) {
    try {
      const stored = window.localStorage.getItem(storageKey);
      if (stored === 'on' || stored === 'off') userMotionChoice = stored;
    } catch {
      // Storage is optional. The control still works for the current page view.
    }
  }

  if (connection?.saveData) root.dataset.lite = 'true';

  const motionToggles = [...document.querySelectorAll('[data-motion-toggle]')];

  function systemRequiresStillness() {
    return reduceQuery.matches || root.dataset.lite === 'true';
  }

  function motionIsOff() {
    if (systemRequiresStillness()) return true;
    if (userMotionChoice === 'off') return true;
    return false;
  }

  function syncMotionState() {
    const isOff = motionIsOff();
    root.dataset.motion = isOff ? 'off' : 'on';
    motionToggles.forEach((toggle) => {
      toggle.setAttribute('aria-pressed', String(isOff));
      toggle.setAttribute('aria-disabled', String(systemRequiresStillness()));
      toggle.setAttribute('aria-label', systemRequiresStillness()
        ? 'Ambient motion is off for reduced-motion or data-saving preferences'
        : isOff ? 'Resume ambient motion' : 'Pause ambient motion');
      const label = toggle.querySelector('[data-motion-label]');
      if (label) label.textContent = isOff ? 'Motion off' : 'Motion on';
    });
    if (isOff) resetSpatialEffects();
    else requestSpatialFrame();
  }

  motionToggles.forEach((toggle) => {
    toggle.addEventListener('click', () => {
      if (systemRequiresStillness()) {
        syncMotionState();
        showToast('Ambient motion remains off for reduced-motion or data-saving preferences.');
        return;
      }
      userMotionChoice = motionIsOff() ? 'on' : 'off';
      if (persistMotionChoice) {
        try { window.localStorage.setItem(storageKey, userMotionChoice); } catch { /* optional */ }
      }
      syncMotionState();
      showToast(motionIsOff() ? 'Ambient motion paused.' : 'Ambient motion restored.');
    });
  });

  const onMotionPreferenceChange = () => syncMotionState();
  if (typeof reduceQuery.addEventListener === 'function') {
    reduceQuery.addEventListener('change', onMotionPreferenceChange);
  }

  const revealTargets = [...document.querySelectorAll('[data-reveal]')];
  if ('IntersectionObserver' in window && !motionIsOff()) {
    const revealObserver = new IntersectionObserver((entries, observer) => {
      entries.forEach((entry) => {
        if (!entry.isIntersecting) return;
        entry.target.classList.add('is-visible');
        observer.unobserve(entry.target);
      });
    }, { rootMargin: '0px 0px -8% 0px', threshold: 0.12 });
    revealTargets.forEach((target) => revealObserver.observe(target));
  } else {
    revealTargets.forEach((target) => target.classList.add('is-visible'));
  }

  const depthLayers = [...document.querySelectorAll('[data-depth]')].map((element) => ({
    element,
    depth: Number.parseFloat(element.dataset.depth || '0') || 0,
  }));
  let targetX = 0;
  let targetY = 0;
  let currentX = 0;
  let currentY = 0;
  let targetScroll = window.scrollY;
  let currentScroll = targetScroll;
  let spatialFrame = 0;
  let documentVisible = !document.hidden;
  let activeTilt = null;
  let pendingTilt = null;
  let pointerX = 0;
  let pointerY = 0;
  let tiltDirty = false;

  function spatialMotionAllowed() {
    return documentVisible && !motionIsOff() && finePointerQuery.matches;
  }

  function resetSpatialEffects() {
    if (spatialFrame) cancelAnimationFrame(spatialFrame);
    spatialFrame = 0;
    currentX = targetX = 0;
    currentY = targetY = 0;
    depthLayers.forEach(({ element }) => {
      element.style.removeProperty('--depth-x');
      element.style.removeProperty('--depth-y');
      element.style.removeProperty('--depth-scroll');
    });
    clearTilt(activeTilt);
    activeTilt = null;
    pendingTilt = null;
    tiltDirty = false;
  }

  function requestSpatialFrame() {
    if (!spatialMotionAllowed() || spatialFrame) return;
    spatialFrame = requestAnimationFrame(updateSpatialFrame);
  }

  function updateSpatialFrame() {
    spatialFrame = 0;
    if (!spatialMotionAllowed()) return;

    currentX += (targetX - currentX) * 0.085;
    currentY += (targetY - currentY) * 0.085;
    currentScroll += (targetScroll - currentScroll) * 0.12;

    if (tiltDirty) updateTilt();

    const scrollDelta = Math.min(32, currentScroll * 0.022);
    depthLayers.forEach(({ element, depth }) => {
      element.style.setProperty('--depth-x', `${(currentX * depth).toFixed(2)}px`);
      element.style.setProperty('--depth-y', `${(currentY * depth).toFixed(2)}px`);
      element.style.setProperty('--depth-scroll', `${(-scrollDelta * depth).toFixed(2)}px`);
    });

    if (
      Math.abs(targetX - currentX) > 0.03 ||
      Math.abs(targetY - currentY) > 0.03 ||
      Math.abs(targetScroll - currentScroll) > 0.2
    ) requestSpatialFrame();
  }

  function updateTilt() {
    tiltDirty = false;
    const nextTilt = pendingTilt;
    if (nextTilt !== activeTilt) {
      clearTilt(activeTilt);
      activeTilt = nextTilt;
    }
    if (!activeTilt || !spatialMotionAllowed()) return;

    const rect = activeTilt.getBoundingClientRect();
    const localX = Math.max(0, Math.min(1, (pointerX - rect.left) / rect.width));
    const localY = Math.max(0, Math.min(1, (pointerY - rect.top) / rect.height));
    activeTilt.style.setProperty('--tilt-y', `${((localX - 0.5) * 7).toFixed(2)}deg`);
    activeTilt.style.setProperty('--tilt-x', `${((0.5 - localY) * 6).toFixed(2)}deg`);
    activeTilt.style.setProperty('--shine-x', `${(localX * 100).toFixed(1)}%`);
    activeTilt.style.setProperty('--shine-y', `${(localY * 100).toFixed(1)}%`);
  }

  function clearTilt(element) {
    if (!element) return;
    element.style.removeProperty('--tilt-x');
    element.style.removeProperty('--tilt-y');
    element.style.removeProperty('--shine-x');
    element.style.removeProperty('--shine-y');
  }

  document.addEventListener('pointermove', (event) => {
    if (!spatialMotionAllowed()) return;
    pendingTilt = event.target instanceof Element ? event.target.closest('[data-tilt]') : null;
    pointerX = event.clientX;
    pointerY = event.clientY;
    tiltDirty = true;
    targetX = ((event.clientX / window.innerWidth) - 0.5) * 52;
    targetY = ((event.clientY / window.innerHeight) - 0.5) * 42;
    requestSpatialFrame();
  }, { passive: true });

  window.addEventListener('blur', () => {
    clearTilt(activeTilt);
    activeTilt = null;
    pendingTilt = null;
    tiltDirty = false;
  });

  window.addEventListener('scroll', () => {
    targetScroll = window.scrollY;
    requestSpatialFrame();
  }, { passive: true });

  document.addEventListener('visibilitychange', () => {
    documentVisible = !document.hidden;
    if (documentVisible) requestSpatialFrame();
    else {
      if (spatialFrame) cancelAnimationFrame(spatialFrame);
      spatialFrame = 0;
      clearTilt(activeTilt);
      activeTilt = null;
    }
  });

  const roomContent = {
    north: {
      title: 'North Clearing',
      description: 'A quiet room for the river recording.',
      artifactKind: 'north',
      artifactLabel: 'Abstract prototype preview of a river field recording',
      mediaLabel: 'river-field-recording.mp4 · sample media',
      mediaIcon: '▶',
      previewLabel: 'Preview sample playback',
      duration: '00:41 / 03:26',
      timelineOffer: 'offer begins · 00:41',
      timelineMoment: 'named moment · 00:52',
      timelineAria: 'Sample artifact path: offer begins at 00:41; named moment at 00:52',
      actionToast: 'Playback is visual only in this design prototype.',
      notes: [
        ['11:40', 'Mira shared a field recording.', '“Morning. I took the recorder down to the river bend.”'],
        ['12:05', 'Apocky offered one interval.', 'Clip 00:41–01:12 · purpose: name the second birdsong · this session only.'],
        ['study', 'Laboratory response preview.', '“The second song enters at 00:52—brighter, descending.” This is authored sample copy, not a live Apocrypha response.'],
      ],
      scopeArtifact: 'clip 00:41–01:12 only',
      scopePurpose: 'name the second birdsong',
      scopeDuration: 'this session',
      offerRoute: 'Apocky → selected clip → Apocrypha',
      offerStatus: 'Prototype state: offered interval is visible; everything else remains outside the membrane.',
    },
    nook: {
      title: 'Reading Nook',
      description: 'Letters, annotations, and quiet continuity.',
      artifactKind: 'nook',
      artifactLabel: 'Abstract prototype preview of a folded sample letter',
      mediaLabel: 'letter-from-the-ridge.txt · sample text',
      mediaIcon: '✦',
      previewLabel: 'Preview sample letter',
      duration: '4 minute read',
      timelineOffer: 'offer begins · ¶2',
      timelineMoment: 'named phrase · ¶4',
      timelineAria: 'Sample letter path: offer begins at paragraph 2; named phrase at paragraph 4',
      actionToast: 'The letter preview is visual only in this design prototype.',
      notes: [
        ['11:47', 'Mira placed a sample letter in the room.', '“The path was silver after rain. I kept the folded map.”'],
        ['12:02', 'Apocky offered one paragraph.', 'Paragraph 2 only · purpose: trace the recurring river image · this session only.'],
        ['study', 'Laboratory response preview.', '“The river returns as a boundary, not a destination.” This is authored sample copy, not a live Apocrypha response.'],
      ],
      scopeArtifact: 'paragraph 2 only',
      scopePurpose: 'trace the recurring river image',
      scopeDuration: 'this session',
      offerRoute: 'Apocky → selected paragraph → Apocrypha',
      offerStatus: 'Prototype state: offered paragraph is visible; everything else remains outside the membrane.',
    },
    workbench: {
      title: 'Workbench',
      description: 'Notes, fragments, and unfinished things with room to breathe.',
      artifactKind: 'workbench',
      artifactLabel: 'Abstract prototype preview of a sample threshold sketch',
      mediaLabel: 'threshold-sketch-07.md · sample notes',
      mediaIcon: '◇',
      previewLabel: 'Preview sample notes',
      duration: '12 fragments',
      timelineOffer: 'offer begins · F04',
      timelineMoment: 'named edge · F07',
      timelineAria: 'Sample note path: offer begins at fragment 4; named edge at fragment 7',
      actionToast: 'The note preview is visual only in this design prototype.',
      notes: [
        ['09:18', 'Apocky placed an unfinished threshold sketch.', '“A door can be visible without becoming an invitation.”'],
        ['09:31', 'Apocky offered one fragment.', 'Fragment 04 only · purpose: test the boundary wording · this session only.'],
        ['study', 'Laboratory response preview.', '“The hinge is agency: the door remains yours to open.” This is authored sample copy, not a live Apocrypha response.'],
      ],
      scopeArtifact: 'fragment 04 only',
      scopePurpose: 'test the boundary wording',
      scopeDuration: 'this session',
      offerRoute: 'Apocky → selected fragment → Apocrypha',
      offerStatus: 'Prototype state: offered fragment is visible; everything else remains outside the membrane.',
    },
  };

  const roomButtons = [...document.querySelectorAll('[data-room]')];
  const roomTitle = document.querySelector('[data-room-title]');
  const roomDescription = document.querySelector('[data-room-description]');
  const artifactPreview = document.querySelector('[data-artifact-preview]');
  const artifactCaption = document.querySelector('[data-artifact-caption]');
  const mediaLabel = document.querySelector('[data-media-label]');
  const mediaDuration = document.querySelector('[data-media-duration]');
  const mediaIcon = document.querySelector('[data-media-icon]');
  const mediaPreviewLabel = document.querySelector('[data-media-preview-label]');
  const mediaAction = document.querySelector('[data-media-action]');
  const artifactTimeline = document.querySelector('[data-artifact-timeline]');
  const timelineOffer = document.querySelector('[data-timeline-offer]');
  const timelineMoment = document.querySelector('[data-timeline-moment]');
  const roomNotes = [...document.querySelectorAll('[data-room-note]')];
  const scopeArtifact = document.querySelector('[data-scope-artifact]');
  const scopePurpose = document.querySelector('[data-scope-purpose]');
  const scopeDuration = document.querySelector('[data-scope-duration]');
  const offerPathLabel = document.querySelector('[data-offer-path-label]');
  const offerPath = document.querySelector('[data-offer-path]');
  let activeRoomKey = 'north';

  function setText(element, value) {
    if (element) element.textContent = value;
  }

  function applyRoomContent(content) {
    setText(roomTitle, content.title);
    setText(roomDescription, content.description);
    if (artifactPreview) {
      artifactPreview.dataset.artifactKind = content.artifactKind;
    }
    setText(artifactCaption, content.artifactLabel);
    setText(mediaLabel, content.mediaLabel);
    setText(mediaDuration, content.duration);
    setText(mediaIcon, content.mediaIcon);
    setText(mediaPreviewLabel, content.previewLabel);
    if (mediaAction) mediaAction.dataset.prototypeAction = content.actionToast;
    if (artifactTimeline) artifactTimeline.setAttribute('aria-label', content.timelineAria);
    setText(timelineOffer, content.timelineOffer);
    setText(timelineMoment, content.timelineMoment);
    roomNotes.forEach((note, index) => {
      const values = content.notes[index];
      if (!values) return;
      setText(note.querySelector('.note-time'), values[0]);
      setText(note.querySelector('strong'), values[1]);
      setText(note.querySelector('p'), values[2]);
    });
    setText(scopeArtifact, content.scopeArtifact);
    setText(scopePurpose, content.scopePurpose);
    setText(scopeDuration, content.scopeDuration);
  }

  function syncOfferPathState(state, content) {
    if (state === 'offered') {
      setText(offerPathLabel, 'Prototype offer path:');
      setText(offerPath, `${content.offerRoute}. The dotted line represents only this scoped prototype offer.`);
      return;
    }
    if (state === 'withdrawn') {
      setText(offerPathLabel, 'Withdrawn path:');
      setText(offerPath, `${content.offerRoute}. No line is active; the room-only receipt remains.`);
      return;
    }
    setText(offerPathLabel, 'Proposed path:');
    setText(offerPath, `${content.offerRoute}. No line is active; offering requires the deliberate action below.`);
  }

  roomButtons.forEach((button) => {
    button.addEventListener('click', () => {
      const room = button.dataset.room;
      const content = roomContent[room];
      if (!content) return;
      const roomChanged = room !== activeRoomKey;
      activeRoomKey = room;
      if (window.location.hash !== `#${room}`) {
        window.history.replaceState(null, '', `#${room}`);
      }
      roomButtons.forEach((item) => {
        const isActive = item === button;
        item.classList.toggle('is-active', isActive);
        item.setAttribute('aria-pressed', String(isActive));
      });
      applyRoomContent(content);
      if (membraneStatus) {
        if (roomChanged) {
          body.dataset.membraneState = 'idle';
          const clearingDraft = document.querySelector('#room-message');
          if (clearingDraft) clearingDraft.value = '';
          if (withdrawLabel) withdrawLabel.textContent = 'Offer with these terms';
          membraneStatus.textContent = 'Prototype state: no offer is active in this room. A prior room receipt would remain in that room\'s history; this static prototype does not persist it.';
        } else {
          membraneStatus.textContent = body.dataset.membraneState === 'offered'
            ? content.offerStatus
            : membraneStatus.textContent;
        }
      }
      syncOfferPathState(body.dataset.membraneState, content);
      showToast(roomChanged
        ? `${content.title} selected. Consent did not carry across rooms.`
        : `${content.title} remains selected. Sample room content stays local.`);
    });
  });

  const withdrawButton = document.querySelector('[data-withdraw]');
  const withdrawLabel = document.querySelector('[data-withdraw-label]');
  const membraneStatus = document.querySelector('[data-membrane-status]');
  withdrawButton?.addEventListener('click', () => {
    const wasOffered = body.dataset.membraneState === 'offered';
    body.dataset.membraneState = wasOffered ? 'withdrawn' : 'offered';
    if (withdrawLabel) withdrawLabel.textContent = wasOffered ? 'Offer with these terms' : 'Withdraw offer';
    if (membraneStatus) {
      membraneStatus.textContent = wasOffered
        ? 'Prototype state: offer withdrawn. The thread is dark; only the room receipt remains.'
        : roomContent[activeRoomKey].offerStatus;
    }
    syncOfferPathState(body.dataset.membraneState, roomContent[activeRoomKey]);
    showToast(wasOffered ? 'Offer withdrawn in the prototype.' : 'Offer created with the visible terms shown here.');
  });

  const requestedRoom = window.location.hash.slice(1);
  const requestedRoomButton = roomButtons.find((button) => button.dataset.room === requestedRoom);
  if (requestedRoomButton && requestedRoom !== activeRoomKey) {
    window.queueMicrotask(() => requestedRoomButton.click());
  }

  const hubRoomContent = {
    north: {
      title: 'North Clearing',
      glyph: '⌁',
      route: '/clearing#north',
      artifactKind: 'north',
      artifactIcon: '▶',
      artifactTitle: 'River field recording',
      artifactMeta: 'clip · 00:41–01:12',
      source: 'Mira',
      object: 'clip 00:41–01:12',
      audience: 'Apocrypha',
      containment: ['interval', 'recording', 'North', 'Clearing'],
      purpose: 'name the second birdsong',
      duration: 'this session',
      messages: [
        ['mira', 'M', 'Mira', 'River recording placed.', '11:40'],
        ['you', 'A', 'Apocky', '00:41–01:12 selected for one question.', '12:05'],
        ['apx', '◈', 'Authored example', '“Second song enters at 00:52.”', 'study'],
      ],
      events: [['11:40', 'placed'], ['12:05', 'selected'], ['study', 'previewed']],
      offerDepth: 'One interval is offered to the named audience; everything else remains outside.',
      offerStatus: 'Sample offer active for clip 00:41–01:12, purpose: name the second birdsong, duration: this session.',
    },
    nook: {
      title: 'Reading Nook',
      glyph: '⌑',
      route: '/clearing#nook',
      artifactKind: 'nook',
      artifactIcon: '✦',
      artifactTitle: 'Letter from the ridge',
      artifactMeta: 'paragraph 2 · 4 minute read',
      source: 'Mira',
      object: 'paragraph 2',
      audience: 'Apocrypha',
      containment: ['paragraph', 'letter', 'Nook', 'Clearing'],
      purpose: 'trace the recurring river image',
      duration: 'this session',
      messages: [
        ['mira', 'M', 'Mira', 'A folded letter was placed.', '11:47'],
        ['you', 'A', 'Apocky', 'Paragraph 2 selected for one question.', '12:02'],
        ['apx', '◈', 'Authored example', '“The river returns as a boundary.”', 'study'],
      ],
      events: [['11:47', 'placed'], ['12:02', 'selected'], ['study', 'previewed']],
      offerDepth: 'One paragraph is offered to the named audience; everything else remains outside.',
      offerStatus: 'Sample offer active for paragraph 2, purpose: trace the recurring river image, duration: this session.',
    },
    workbench: {
      title: 'Workbench',
      glyph: '◇',
      route: '/clearing#workbench',
      artifactKind: 'workbench',
      artifactIcon: '◇',
      artifactTitle: 'Threshold sketch 07',
      artifactMeta: 'fragment 04 · 12 fragments',
      source: 'Apocky',
      object: 'fragment 04',
      audience: 'Apocrypha',
      containment: ['fragment', 'sketch', 'Workbench', 'Clearing'],
      purpose: 'test the boundary wording',
      duration: 'this session',
      messages: [
        ['you', 'A', 'Apocky', 'An unfinished threshold sketch was placed.', '09:18'],
        ['you', 'A', 'Apocky', 'Fragment 04 selected for one question.', '09:31'],
        ['apx', '◈', 'Authored example', '“The hinge is agency.”', 'study'],
      ],
      events: [['09:18', 'placed'], ['09:31', 'selected'], ['study', 'previewed']],
      offerDepth: 'One fragment is offered to the named audience; everything else remains outside.',
      offerStatus: 'Sample offer active for fragment 04, purpose: test the boundary wording, duration: this session.',
    },
  };

  const hubRoomButtons = [...document.querySelectorAll('[data-hub-room]')];
  const hubRoomTitle = document.querySelector('[data-hub-room-title]');
  const hubRoomGlyph = document.querySelector('[data-hub-room-glyph]');
  const hubEnter = document.querySelector('[data-hub-enter]');
  const hubArtifact = document.querySelector('.shared-object');
  const hubArtifactIcon = document.querySelector('[data-hub-artifact-icon]');
  const hubArtifactTitle = document.querySelector('[data-hub-artifact-title]');
  const hubArtifactMeta = document.querySelector('[data-hub-artifact-meta]');
  const hubMessages = document.querySelector('[data-hub-messages]');
  const hubLineage = document.querySelector('[data-hub-lineage]');
  const hubAxisTitle = document.querySelector('[data-hub-axis-title]');
  const axisSource = document.querySelector('[data-axis-source]');
  const axisObject = document.querySelector('[data-axis-object]');
  const axisAudience = document.querySelector('[data-axis-audience]');
  const axisContainment = document.querySelector('[data-axis-containment]');
  const axisEvents = document.querySelector('[data-axis-events]');
  const axisPurpose = document.querySelector('[data-axis-purpose]');
  const axisDuration = document.querySelector('[data-axis-duration]');
  const axisDepth = document.querySelector('[data-axis-depth]');
  const axisOfferPlane = document.querySelector('[data-axis-offer-plane]');
  const hubOfferButtons = [...document.querySelectorAll('[data-hub-offer], [data-mobile-offer]')];
  const hubOfferLabels = [...document.querySelectorAll('[data-hub-offer-label]')];
  const hubOfferStatus = document.querySelector('[data-hub-offer-status]');
  const hubMobileOfferState = document.querySelector('[data-mobile-offer-state]');
  const hubStatus = document.querySelector('[data-hub-status]');
  const hubComposer = document.querySelector('[data-hub-composer]');
  const hubInput = document.querySelector('[data-hub-input]');
  let activeHubRoom = 'north';
  let hubOfferState = 'idle';

  function createMessage(values) {
    const [speaker, avatar, author, message, time] = values;
    const item = document.createElement('li');
    item.dataset.speaker = speaker;
    const avatarNode = document.createElement('span');
    avatarNode.className = 'message-avatar';
    avatarNode.textContent = avatar;
    const copy = document.createElement('p');
    const strong = document.createElement('strong');
    strong.textContent = author;
    const messageNode = document.createElement('span');
    messageNode.textContent = message;
    copy.append(strong, messageNode);
    const timeNode = document.createElement('time');
    timeNode.textContent = time;
    item.append(avatarNode, copy, timeNode);
    return item;
  }

  function renderHubMessages(content) {
    if (!hubMessages) return;
    hubMessages.replaceChildren(...content.messages.map(createMessage));
  }

  function renderHubLineage(content) {
    if (!hubLineage) return;
    const steps = [...hubLineage.querySelectorAll('.lineage-step')];
    steps.forEach((step, index) => {
      const values = content.events[index];
      const time = step.querySelector('small');
      const label = step.querySelector('strong');
      if (values) {
        setText(time, values[0]);
        setText(label, values[1]);
        step.classList.toggle('is-current', index === content.events.length - 1);
        step.classList.toggle('is-complete', index < content.events.length - 1);
      } else {
        setText(time, '—');
        setText(label, 'receipt');
        step.classList.remove('is-current', 'is-complete');
      }
    });
  }

  function renderAxisList(container, values, template) {
    if (!container) return;
    container.replaceChildren(...values.map((value) => {
      const item = document.createElement('li');
      if (template === 'events') {
        const time = document.createElement('small');
        time.textContent = value[0];
        const label = document.createElement('strong');
        label.textContent = value[1];
        item.append(time, label);
      } else {
        item.textContent = value;
      }
      return item;
    }));
  }

  function syncHubOfferState(state, announcement) {
    if (!hubRoomButtons.length) return;
    const content = hubRoomContent[activeHubRoom];
    hubOfferState = state;
    body.dataset.hubOfferState = state;
    const isOffered = state === 'offered';
    setText(hubOfferStatus, isOffered ? 'sample offer active' : 'no sample offer');
    setText(hubMobileOfferState, isOffered ? 'sample offer active' : 'no sample offer');
    hubOfferButtons.forEach((button) => {
      const label = button.matches('[data-mobile-offer]') ? button : button.querySelector('[data-hub-offer-label]');
      setText(label, isOffered ? 'Withdraw sample' : 'Offer sample');
    });
    hubOfferLabels.forEach((label) => setText(label, isOffered ? 'Withdraw sample' : 'Offer sample'));
    if (axisOfferPlane) {
      axisOfferPlane.textContent = isOffered ? 'offered' : 'proposed';
      axisOfferPlane.classList.toggle('is-active', isOffered);
      const roomPlane = axisOfferPlane.previousElementSibling;
      roomPlane?.classList.toggle('is-active', !isOffered);
    }
    setText(axisDepth, isOffered
      ? content.offerDepth
      : 'No offer is active; the named audience is only a proposed path.');
    if (hubStatus) {
      hubStatus.textContent = announcement || (isOffered
        ? content.offerStatus
        : 'No offer is active. The sample path is only proposed.');
    }
  }

  function renderHubRoom(roomKey, announce = true) {
    const content = hubRoomContent[roomKey];
    if (!content || !hubRoomButtons.length) return;
    const roomChanged = roomKey !== activeHubRoom;
    activeHubRoom = roomKey;
    hubRoomButtons.forEach((button) => {
      const selected = button.dataset.hubRoom === roomKey;
      button.classList.toggle('is-active', selected);
      button.setAttribute('aria-pressed', String(selected));
    });
    setText(hubRoomTitle, content.title);
    setText(hubRoomGlyph, content.glyph);
    if (hubEnter) hubEnter.href = content.route;
    if (hubArtifact) hubArtifact.dataset.hubArtifactKind = content.artifactKind;
    setText(hubArtifactIcon, content.artifactIcon);
    setText(hubArtifactTitle, content.artifactTitle);
    setText(hubArtifactMeta, content.artifactMeta);
    setText(hubAxisTitle, content.artifactTitle);
    setText(axisSource, content.source);
    setText(axisObject, content.object);
    setText(axisAudience, content.audience);
    setText(axisPurpose, content.purpose);
    setText(axisDuration, content.duration);
    renderAxisList(axisContainment, content.containment);
    renderAxisList(axisEvents, content.events, 'events');
    renderHubMessages(content);
    renderHubLineage(content);
    if (hubInput && roomChanged) hubInput.value = '';
    syncHubOfferState('idle', announce
      ? `${content.title} selected. No sample offer or local draft carried across rooms.`
      : undefined);
  }

  hubRoomButtons.forEach((button) => {
    button.addEventListener('click', () => renderHubRoom(button.dataset.hubRoom));
  });

  hubOfferButtons.forEach((button) => {
    button.addEventListener('click', () => {
      const isOffered = hubOfferState === 'offered';
      syncHubOfferState(
        isOffered ? 'idle' : 'offered',
        isOffered
          ? 'Sample offer withdrawn. The path is no longer active.'
          : hubRoomContent[activeHubRoom].offerStatus,
      );
    });
  });

  hubComposer?.addEventListener('submit', (event) => {
    event.preventDefault();
    const message = hubInput?.value.trim();
    if (!message || !hubMessages) {
      showToast('Write something first. Nothing is sent from this prototype.');
      return;
    }
    hubMessages.append(createMessage(['you', 'A', 'You · local', message, 'now']));
    if (hubInput) hubInput.value = '';
    hubMessages.lastElementChild?.scrollIntoView({ block: 'nearest' });
    showToast('Local only · nothing sent or stored.');
  });

  const hubViewButtons = [...document.querySelectorAll('[data-hub-view]')];
  hubViewButtons.forEach((button) => {
    button.addEventListener('click', () => {
      const view = button.dataset.hubView;
      body.dataset.hubViewMode = view;
      hubViewButtons.forEach((item) => item.setAttribute('aria-pressed', String(item === button)));
    });
  });

  const ontologyPanel = document.querySelector('.ontology-panel');
  const ontologyButtons = [...document.querySelectorAll('[data-ontology-focus]')];
  const axisCode = document.querySelector('[data-axis-code]');
  const axisLabels = {
    all: 'People · Meaning · Visibility · Time',
    x: 'People · source → item → audience',
    y: 'Meaning · fragment → system',
    z: 'Visibility · local → offered',
    t: 'Time · ordered history',
  };

  function selectOntologyLens(button) {
    const lens = button.dataset.ontologyFocus;
    if (!lens || !ontologyPanel) return;
    ontologyPanel.dataset.ontologyLens = lens;
    ontologyButtons.forEach((item) => item.setAttribute('aria-pressed', String(item === button)));
    setText(axisCode, axisLabels[lens]);
  }

  ontologyButtons.forEach((button, index) => {
    button.addEventListener('click', () => selectOntologyLens(button));
    button.addEventListener('keydown', (event) => {
      let nextIndex = null;
      if (event.key === 'ArrowRight' || event.key === 'ArrowDown') nextIndex = (index + 1) % ontologyButtons.length;
      if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') nextIndex = (index - 1 + ontologyButtons.length) % ontologyButtons.length;
      if (event.key === 'Home') nextIndex = 0;
      if (event.key === 'End') nextIndex = ontologyButtons.length - 1;
      if (nextIndex === null) return;
      event.preventDefault();
      ontologyButtons[nextIndex].focus();
      selectOntologyLens(ontologyButtons[nextIndex]);
    });
  });

  const pathIndex = document.querySelector('[data-path-index]');
  const pathIndexOpeners = [...document.querySelectorAll('[data-index-open]')];
  const pathIndexCloser = pathIndex?.querySelector('[data-index-close]');
  let pathIndexTrigger = null;

  function openPathIndex(trigger) {
    if (!pathIndex) return;
    pathIndexTrigger = trigger;
    if (typeof pathIndex.showModal === 'function') pathIndex.showModal();
    else pathIndex.setAttribute('open', '');
  }

  function closePathIndex() {
    if (!pathIndex) return;
    if (typeof pathIndex.close === 'function') pathIndex.close();
    else pathIndex.removeAttribute('open');
  }

  pathIndexOpeners.forEach((button) => button.addEventListener('click', () => openPathIndex(button)));
  pathIndexCloser?.addEventListener('click', closePathIndex);
  pathIndex?.addEventListener('close', () => pathIndexTrigger?.focus());
  pathIndex?.addEventListener('click', (event) => {
    if (event.target === pathIndex) closePathIndex();
  });

  document.addEventListener('keydown', (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      if (pathIndex?.open) closePathIndex();
      else openPathIndex(pathIndexOpeners[0]);
    }
  });

  if (hubRoomButtons.length) {
    body.dataset.hubOfferState = 'idle';
    renderHubRoom('north', false);
  }

  document.querySelectorAll('[data-prototype-action]').forEach((control) => {
    control.addEventListener('click', () => {
      showToast(control.dataset.prototypeAction || 'This is a design-only interaction.');
    });
  });

  document.querySelectorAll('[data-prototype-form]').forEach((form) => {
    form.addEventListener('submit', (event) => {
      event.preventDefault();
      showToast('Prototype only—your draft stayed here and was not sent.');
    });
  });

  const hubInlineStatus = document.querySelector('[data-hub-inline-status]');
  const toast = document.querySelector('[data-toast]');
  let toastTimer = 0;
  let hubInlineTimer = 0;
  function showToast(message) {
    if (!message) return;
    if (hubInlineStatus) {
      hubInlineStatus.textContent = message;
      hubInlineStatus.title = message;
      window.clearTimeout(hubInlineTimer);
      hubInlineTimer = window.setTimeout(() => {
        hubInlineStatus.textContent = '◇ local draft · never sent';
        hubInlineStatus.removeAttribute('title');
      }, 3200);
      return;
    }
    if (!toast) return;
    toast.textContent = message;
    toast.classList.add('is-visible');
    window.clearTimeout(toastTimer);
    toastTimer = window.setTimeout(() => toast.classList.remove('is-visible'), 3200);
  }

  syncMotionState();
  requestSpatialFrame();
})();
