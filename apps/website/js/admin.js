// Queue admin: Google sign-in through Supabase Auth (PKCE), then the alpha-admin function.
// The access token lives only in this page's memory; a reload asks for sign-in again.
// Only accounts listed in public.alpha_admins get any data back. No cookies, no tracking.
(function () {
  'use strict';
  var SB = 'https://wmmvawvfnwdfzxvumlfc.supabase.co';
  var KEY = 'sb_publishable_ReyVHSfs29Wzq_n49Pq_2A_y_A7qke9'; // publishable key, safe in a browser
  var VERIFIER = 'kohbor.admin.pkce';
  var root = document.documentElement;
  var $ = function (id) { return document.getElementById(id); };
  var store = {
    get: function (s, k) { try { return s.getItem(k); } catch (e) { return null; } },
    set: function (s, k, v) { try { s.setItem(k, v); } catch (e) { /* private mode */ } },
    del: function (s, k) { try { s.removeItem(k); } catch (e) { /* private mode */ } }
  };
  var lang = store.get(window.localStorage, 'kohbor.lang') === 'en' ? 'en' : 'th';
  var T = {
    redirect: ['กำลังไปที่ Google…', 'Going to Google…'],
    loading: ['กำลังโหลด…', 'Loading…'],
    cancelled: ['เข้าสู่ระบบไม่สำเร็จ ลองใหม่อีกครั้ง', 'Sign-in failed. Try again.'],
    expired: ['หมดเวลาเข้าสู่ระบบ กดเข้าสู่ระบบอีกครั้ง', 'Your sign-in expired. Sign in again.'],
    forbidden: ['บัญชีนี้ไม่ใช่ผู้ดูแล', 'This account is not an admin.'],
    unavailable: ['ระบบไม่ตอบ ลองใหม่อีกครั้ง', 'The service did not answer. Try again.'],
    nocrypto: ['เบราว์เซอร์นี้ใช้ไม่ได้ ลองเบราว์เซอร์อื่น', 'This browser cannot sign in. Try another one.'],
    none: ['ไม่มีใครรอคิวอยู่', 'Nobody is waiting.'],
    copied: ['คัดลอกอีเมล {n} คนแล้ว วางในช่อง BCC ได้เลย', 'Copied {n} emails. Paste them into BCC.'],
    copyfail: ['คัดลอกอัตโนมัติไม่ได้ เลือกข้อความในช่องด้านล่างแล้วคัดลอกเอง', 'Could not copy. Select the text below and copy it.'],
    marked: ['บันทึกแล้ว: เชิญ {n} คน', 'Saved: {n} invited.'],
    invited: ['เชิญแล้ว', 'Invited'],
    waiting: ['รอคิว', 'Waiting'],
    doInvite: ['เชิญแล้ว', 'Mark invited'],
    undo: ['กลับเข้าคิว', 'Back to queue'],
    sendOne: ['ส่งคำเชิญ', 'Send invite'],
    resend: ['ส่งลิงก์ใหม่', 'Send new link'],
    confirmSend: ['ส่งอีเมลเชิญพร้อมลิงก์ดาวน์โหลดให้ {n} คน จาก Gmail ที่ตั้งไว้?', 'Email {n} people their own download link from the configured Gmail?'],
    confirmResend: ['ส่งลิงก์ใหม่ให้คนนี้? ลิงก์เก่าของเขาจะใช้ไม่ได้อีก', 'Send this person a new link? Their old link will stop working.'],
    sending: ['กำลังส่ง… {n} คน', 'Sending… {n} left'],
    sentAll: ['ส่งคำเชิญแล้ว {n} คน', 'Sent {n} invites.'],
    sentSome: ['ส่งแล้ว {n} คน บางคนส่งไม่สำเร็จ (ยังอยู่ในคิว ลองใหม่ได้)', 'Sent {n}. Some could not be sent and stay in the queue; try again.'],
    mail_not_configured: ['ยังไม่ได้ตั้งค่า Gmail', 'Gmail is not set up.'],
    mail_login: ['Gmail ไม่ยอมให้เข้าสู่ระบบ ตรวจ App Password ใน Supabase Secrets', 'Gmail refused the sign-in. Check the App Password in Supabase Secrets.'],
    mail_failed: ['ส่งอีเมลไม่สำเร็จ ลองใหม่อีกครั้ง', 'Sending failed. Try again.'],
    bad_request: ['ข้อมูลไม่ถูกต้อง', 'That was not accepted.'],
    tplSaved: ['บันทึกเทมเพลตแล้ว', 'Template saved.'],
    tplNeedsLink: ['เนื้อหาต้องมี {link}', 'The body must contain {link}.'],
    testSent: ['ส่งอีเมลทดสอบแล้ว ดูในกล่องจดหมายของคุณ', 'Test sent. Check your inbox.']
  };
  function t(k, n) { return T[k][lang === 'th' ? 0 : 1].replace('{n}', String(n)); }
  var shown = null;
  function say(k) { shown = k; $('adMsg').textContent = k ? t(k) : ''; }

  var token = null, queue = [], links = [], batch = [], batchNote = null, mailReady = false, busy = false;
  var notes = {};

  function setLang(l) {
    lang = l; root.dataset.lang = l; root.lang = l; store.set(window.localStorage, 'kohbor.lang', l);
    Array.prototype.forEach.call(document.querySelectorAll('[data-set-lang]'), function (b) { b.setAttribute('aria-pressed', String(b.dataset.setLang === l)); });
    if (shown) say(shown);
    if (batchNote) $('adBatchMsg').textContent = t(batchNote[0], batchNote[1]);
    Object.keys(notes).forEach(function (id) { if (notes[id]) $(id).textContent = t(notes[id][0], notes[id][1]); });
    render();
  }
  Array.prototype.forEach.call(document.querySelectorAll('[data-set-lang]'), function (b) { b.addEventListener('click', function () { setLang(b.dataset.setLang); }); });

  function b64url(bytes) {
    var s = ''; for (var i = 0; i < bytes.length; i++) s += String.fromCharCode(bytes[i]);
    return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  }
  function fmt(iso) {
    var d = new Date(iso); if (isNaN(d)) return '';
    return d.toLocaleString(lang === 'th' ? 'th-TH' : 'en-GB', { dateStyle: 'medium', timeStyle: 'short' });
  }
  function note(k, n) { batchNote = k ? [k, n] : null; $('adBatchMsg').textContent = k ? t(k, n) : ''; }

  function signedOut(k) {
    token = null; $('adPanel').hidden = true; $('adSignin').hidden = false; $('adGoogle').disabled = false; say(k);
  }

  function call(body) {
    return fetch(SB + '/functions/v1/alpha-admin', {
      method: 'POST',
      headers: { 'content-type': 'application/json', apikey: KEY, authorization: 'Bearer ' + token },
      body: JSON.stringify(body),
      credentials: 'omit'
    }).then(function (r) {
      return r.json().catch(function () { return {}; }).then(function (b) {
        if (r.ok) return b;
        var known = { mail_not_configured: 1, mail_login: 1, mail_failed: 1, bad_request: 1 };
        throw new Error(r.status === 401 ? 'expired' : r.status === 403 ? 'forbidden' : known[b && b.error] ? b.error : 'unavailable');
      });
    }, function () { throw new Error('unavailable'); });
  }
  function fail(e) {
    var k = e && e.message;
    if (k === 'expired' || k === 'forbidden') signedOut(k); else say('unavailable');
  }
  // A status line under one section, kept across language switches.
  function noteIn(id, k, n) { notes[id] = k ? [k, n] : null; $(id).textContent = k ? t(k, n) : ''; }
  function failIn(id) {
    return function (e) {
      var k = e && e.message;
      if (k === 'expired' || k === 'forbidden') { signedOut(k); return; }
      noteIn(id, T[k] ? k : 'unavailable');
    };
  }

  function load() {
    say('loading');
    return call({ action: 'queue' }).then(function (b) {
      queue = Array.isArray(b.queue) ? b.queue : [];
      links = Array.isArray(b.links) ? b.links : [];
      mailReady = b.mail_ready === true;
      if (b.template && !$('adSubject').dataset.loaded) {
        $('adSubject').value = b.template.subject || '';
        $('adBody').value = b.template.body || '';
        $('adSubject').dataset.loaded = '1';
      }
      $('adSignin').hidden = true; $('adPanel').hidden = false;
      say(null); render();
    }).catch(fail);
  }

  function render() {
    if (!token) return;
    var waiting = queue.filter(function (q) { return !q.invited_at; });
    $('stWaiting').textContent = String(waiting.length);
    $('stInvited').textContent = String(queue.length - waiting.length);
    $('adMailSetup').hidden = mailReady;
    $('adSend').disabled = !mailReady || busy || !waiting.length;
    $('adTest').disabled = !mailReady || busy;
    $('stDownloads').textContent = String(links.reduce(function (s, l) { return s + (Number(l.downloads) || 0); }, 0));
    var body = $('adRows'); body.textContent = '';
    queue.forEach(function (q) {
      var tr = document.createElement('tr');
      function cell(text, cls) { var td = document.createElement('td'); if (cls) td.className = cls; td.textContent = text; tr.appendChild(td); return td; }
      cell(String(q.queue_no), 'num');
      cell(q.email || '—', 'email');
      cell(q.name || '—');
      cell(fmt(q.joined_at));
      var st = cell('');
      var pill = document.createElement('span');
      pill.className = 'ad-pill' + (q.invited_at ? ' on' : '');
      pill.textContent = q.invited_at ? t('invited') + ' · ' + fmt(q.invited_at) : t('waiting');
      st.appendChild(pill);
      cell(String(Number(q.downloads) || 0), 'num');
      var act = cell(''); var acts = document.createElement('div'); acts.className = 'ad-acts'; act.appendChild(acts);
      if (mailReady && q.email) {
        var s1 = document.createElement('button');
        s1.type = 'button'; s1.className = 'ad-btn ad-small'; s1.disabled = busy;
        s1.textContent = q.invited_at ? t('resend') : t('sendOne');
        s1.addEventListener('click', function () {
          if (q.invited_at && !window.confirm(t('confirmResend'))) return;
          sendInvites([q.user_id]);
        });
        acts.appendChild(s1);
      }
      var b = document.createElement('button');
      b.type = 'button'; b.className = 'ad-btn ad-quiet ad-small';
      b.textContent = q.invited_at ? t('undo') : t('doInvite');
      b.addEventListener('click', function () { setInvited([q.user_id], !q.invited_at, b); });
      acts.appendChild(b);
      body.appendChild(tr);
    });
    $('adEmpty').hidden = queue.length > 0;
  }

  function setInvited(ids, invited, btn) {
    if (btn) btn.disabled = true;
    return call({ action: 'set_invited', users: ids, invited: invited }).then(function (b) {
      var rows = Array.isArray(b.changed) ? b.changed : [];
      var changed = {};
      rows.forEach(function (c) { changed[c.user_id] = c.invited_at; });
      queue.forEach(function (q) { if (Object.prototype.hasOwnProperty.call(changed, q.user_id)) q.invited_at = changed[q.user_id]; });
      render();
      return rows.length;
    }).catch(function (e) { if (btn) btn.disabled = false; fail(e); return -1; });
  }

  $('adCopy').addEventListener('click', function () {
    var n = Math.max(1, Math.min(500, parseInt($('adCount').value, 10) || 0));
    var pick = queue.filter(function (q) { return !q.invited_at && q.email; }).slice(0, n);
    var box = $('adEmails');
    if (!pick.length) { batch = []; $('adMark').disabled = true; box.hidden = true; note('none'); return; }
    batch = pick.map(function (q) { return q.user_id; });
    var text = pick.map(function (q) { return q.email; }).join(', ');
    box.value = text; box.hidden = false;
    $('adMark').disabled = false;
    var copyFail = function () { box.focus(); box.select(); note('copyfail'); };
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(function () { note('copied', pick.length); }, copyFail);
    } else { copyFail(); }
  });

  $('adMark').addEventListener('click', function () {
    if (!batch.length) return;
    var btn = $('adMark');
    setInvited(batch, true, btn).then(function (n) {
      if (n < 0) return;
      batch = []; btn.disabled = true; $('adEmails').hidden = true; note('marked', n);
    });
  });

  // Send in groups of 20 (the function's limit per request), one group after another.
  function sendInvites(ids) {
    if (busy || !ids.length) return;
    busy = true; render();
    var sent = 0, failed = 0, rest = ids.slice();
    function next() {
      if (!rest.length) return Promise.resolve();
      var group = rest.splice(0, 20);
      noteIn('adSendMsg', 'sending', rest.length + group.length);
      return call({ action: 'invite', users: group }).then(function (b) {
        var changed = {};
        (b.changed || []).forEach(function (c) { changed[c.user_id] = c.invited_at; });
        queue.forEach(function (q) { if (Object.prototype.hasOwnProperty.call(changed, q.user_id)) q.invited_at = changed[q.user_id]; });
        sent += (b.sent || []).length; failed += (b.failed || []).length;
        render();
        if ((b.failed || []).length) { failed += rest.length; rest = []; return; }
        return next();
      });
    }
    next().then(function () {
      noteIn('adSendMsg', failed ? 'sentSome' : 'sentAll', sent);
    }, failIn('adSendMsg')).then(function () { busy = false; load(); });
  }

  $('adSend').addEventListener('click', function () {
    var n = Math.max(1, Math.min(200, parseInt($('adSendCount').value, 10) || 0));
    var ids = queue.filter(function (q) { return !q.invited_at && q.email; }).slice(0, n).map(function (q) { return q.user_id; });
    if (!ids.length) { noteIn('adSendMsg', 'none'); return; }
    if (!window.confirm(t('confirmSend', ids.length))) return;
    sendInvites(ids);
  });

  $('adSaveTpl').addEventListener('click', function () {
    var subject = $('adSubject').value.trim(), body = $('adBody').value;
    if (body.indexOf('{link}') < 0) { noteIn('adTplMsg', 'tplNeedsLink'); return; }
    var btn = $('adSaveTpl'); btn.disabled = true;
    call({ action: 'set_template', subject: subject, body: body }).then(function () { noteIn('adTplMsg', 'tplSaved'); }, failIn('adTplMsg'))
      .then(function () { btn.disabled = false; });
  });

  $('adTest').addEventListener('click', function () {
    var subject = $('adSubject').value.trim(), body = $('adBody').value;
    if (body.indexOf('{link}') < 0) { noteIn('adTplMsg', 'tplNeedsLink'); return; }
    var btn = $('adTest'); btn.disabled = true;
    // Save first, so the test shows exactly what is on screen.
    call({ action: 'set_template', subject: subject, body: body }).then(function () { return call({ action: 'test_mail' }); }).then(function () { noteIn('adTplMsg', 'testSent'); }, failIn('adTplMsg'))
      .then(function () { btn.disabled = !mailReady; });
  });

  $('adRefresh').addEventListener('click', function () { load(); });

  $('adGoogle').addEventListener('click', function () {
    if (!window.crypto || !crypto.subtle) { say('nocrypto'); return; }
    var verifier = b64url(crypto.getRandomValues(new Uint8Array(48)));
    store.set(window.sessionStorage, VERIFIER, verifier);
    $('adGoogle').disabled = true; say('redirect');
    crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier)).then(function (d) {
      location.assign(SB + '/auth/v1/authorize?provider=google' +
        '&redirect_to=' + encodeURIComponent(location.origin + location.pathname) +
        '&code_challenge=' + b64url(new Uint8Array(d)) + '&code_challenge_method=s256');
    }).catch(function () { $('adGoogle').disabled = false; say('nocrypto'); });
  });

  setLang(lang);

  // Coming back from Google.
  var q = new URLSearchParams(location.search);
  var code = q.get('code');
  var failed = q.get('error') || q.get('error_description');
  if (code || failed || location.hash.indexOf('error') >= 0) history.replaceState(null, '', location.pathname);
  if (failed || (!code && location.hash.indexOf('error') >= 0)) { say('cancelled'); return; }
  if (!code) return;
  var verifier = store.get(window.sessionStorage, VERIFIER);
  store.del(window.sessionStorage, VERIFIER);
  if (!verifier) { say('expired'); return; }
  $('adGoogle').disabled = true; say('loading');
  fetch(SB + '/auth/v1/token?grant_type=pkce', {
    method: 'POST',
    headers: { 'content-type': 'application/json', apikey: KEY },
    body: JSON.stringify({ auth_code: code, code_verifier: verifier }),
    credentials: 'omit'
  }).then(function (res) {
    return res.json().catch(function () { return {}; }).then(function (s) {
      if (!res.ok || !s || typeof s.access_token !== 'string') throw new Error('expired');
      token = s.access_token;
      return load();
    });
  }).catch(function (e) { signedOut(e && e.message === 'expired' ? 'expired' : 'unavailable'); });
})();
