// Alpha sign-up: Google sign-in through Supabase Auth (PKCE), then join the test queue.
// The access token stays in memory for one request and is never stored. No cookies, no tracking.
(function () {
  'use strict';
  var SB = 'https://wmmvawvfnwdfzxvumlfc.supabase.co';
  var KEY = 'sb_publishable_ReyVHSfs29Wzq_n49Pq_2A_y_A7qke9'; // publishable key, safe in a browser
  var VERIFIER = 'kohbor.pkce';
  var root = document.documentElement;
  var $ = function (id) { return document.getElementById(id); };
  var store = {
    get: function (s, k) { try { return s.getItem(k); } catch (e) { return null; } },
    set: function (s, k, v) { try { s.setItem(k, v); } catch (e) { /* private mode */ } },
    del: function (s, k) { try { s.removeItem(k); } catch (e) { /* private mode */ } }
  };
  var lang = store.get(window.localStorage, 'kohbor.lang') === 'en' ? 'en' : 'th';

  var TEXT = {
    working: { th: 'กำลังลงทะเบียน…', en: 'Signing you up…' },
    redirect: { th: 'กำลังไปที่ Google…', en: 'Going to Google…' },
    cancelled: { th: 'ยังไม่ได้ลงทะเบียน การลงชื่อเข้าใช้ Google ถูกยกเลิกหรือไม่สำเร็จ ลองใหม่อีกครั้ง', en: 'Not signed up. Google sign-in was cancelled or failed. Try again.' },
    expired: { th: 'การลงชื่อเข้าใช้หมดเวลา ลองกดลงทะเบียนอีกครั้ง', en: 'The sign-in expired. Press sign up again.' },
    unavailable: { th: 'ระบบไม่ตอบ ลองใหม่อีกครั้งภายหลัง', en: 'The service did not answer. Try again later.' },
    nocrypto: { th: 'เบราว์เซอร์นี้ใช้ลงทะเบียนไม่ได้ ลองเบราว์เซอร์อื่น', en: 'This browser cannot sign up. Try another browser.' }
  };
  var shown = null;
  function say(key) { shown = key; $('rgMsg').textContent = key ? TEXT[key][lang] : ''; }

  var done = null; // { email, position, invited }
  function renderDone() {
    if (!done) return;
    var p = $('rgDoneText');
    p.textContent = '';
    var email = document.createElement('b');
    email.textContent = done.email || (lang === 'th' ? 'อีเมลของคุณ' : 'your email');
    if (done.invited) {
      p.append(lang === 'th' ? 'คุณได้รับเชิญแล้ว ลิงก์ดาวน์โหลดถูกส่งไปที่ ' : 'You have been invited. The download link was sent to ', email, lang === 'th' ? ' ถ้าไม่เห็น ลองดูในโฟลเดอร์สแปม' : '. If you cannot see it, check your spam folder.');
    } else {
      var place = typeof done.position === 'number' ? String(done.position) : '—';
      p.append(lang === 'th' ? 'คุณอยู่ในคิวลำดับที่ ' : 'You are number ', place,
        lang === 'th' ? ' ลิงก์ดาวน์โหลดจะส่งไปที่ ' : ' in the queue. The download link will be sent to ', email,
        lang === 'th' ? ' ตามคิวเข้าทดสอบ' : ' when your turn comes.');
    }
    $('rgStart').hidden = true;
    $('rgDone').hidden = false;
  }

  function setLang(l) {
    lang = l; root.dataset.lang = l; root.lang = l; store.set(window.localStorage, 'kohbor.lang', l);
    Array.prototype.forEach.call(document.querySelectorAll('[data-set-lang]'), function (b) { b.setAttribute('aria-pressed', String(b.dataset.setLang === l)); });
    if (shown) say(shown);
    renderDone();
  }
  Array.prototype.forEach.call(document.querySelectorAll('[data-set-lang]'), function (b) { b.addEventListener('click', function () { setLang(b.dataset.setLang); }); });
  setLang(lang);

  function b64url(bytes) {
    var s = ''; for (var i = 0; i < bytes.length; i++) s += String.fromCharCode(bytes[i]);
    return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  }

  var agree = $('rgAgree'), go = $('rgGoogle');
  agree.addEventListener('change', function () { go.disabled = !agree.checked; });

  go.addEventListener('click', function () {
    if (!agree.checked) return;
    if (!window.crypto || !crypto.subtle) { say('nocrypto'); return; }
    var verifier = b64url(crypto.getRandomValues(new Uint8Array(48)));
    store.set(window.sessionStorage, VERIFIER, verifier);
    go.disabled = true;
    say('redirect');
    crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier)).then(function (d) {
      var redirect = location.origin + location.pathname;
      location.assign(SB + '/auth/v1/authorize?provider=google' +
        '&redirect_to=' + encodeURIComponent(redirect) +
        '&code_challenge=' + b64url(new Uint8Array(d)) + '&code_challenge_method=s256');
    }).catch(function () { go.disabled = false; say('nocrypto'); });
  });

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
  agree.checked = true;
  say('working');

  fetch(SB + '/auth/v1/token?grant_type=pkce', {
    method: 'POST',
    headers: { 'content-type': 'application/json', apikey: KEY },
    body: JSON.stringify({ auth_code: code, code_verifier: verifier }),
    credentials: 'omit'
  }).then(function (res) {
    return res.json().catch(function () { return {}; }).then(function (s) {
      if (!res.ok || !s || typeof s.access_token !== 'string') throw new Error('signin');
      var email = s.user && typeof s.user.email === 'string' ? s.user.email : null;
      return fetch(SB + '/functions/v1/alpha-waitlist', {
        method: 'POST',
        headers: { 'content-type': 'application/json', apikey: KEY, authorization: 'Bearer ' + s.access_token },
        body: '{}',
        credentials: 'omit'
      }).then(function (r) {
        return r.json().catch(function () { return {}; }).then(function (w) {
          if (!r.ok) throw new Error(r.status === 401 ? 'signin' : 'unavailable');
          done = { email: email, position: typeof w.queue_position === 'number' ? w.queue_position : null, invited: w.invited === true };
          say(null);
          renderDone();
        });
      });
    });
  }).catch(function (e) { say(e && e.message === 'signin' ? 'expired' : 'unavailable'); });
})();
