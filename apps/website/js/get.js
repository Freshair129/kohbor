// Shared download link: /get.html#<link key>. The key is in the fragment, so it never reaches a server log;
// it is sent only to the download function, which answers with a short-lived installer link.
(function () {
  'use strict';
  var ENDPOINT = 'https://wmmvawvfnwdfzxvumlfc.supabase.co/functions/v1/alpha-download';
  var root = document.documentElement;
  var msg = document.getElementById('dlMsg');
  var go = document.getElementById('dlGo');
  var lang = 'th';
  try { lang = localStorage.getItem('kohbor.lang') === 'en' ? 'en' : 'th'; } catch (e) { /* private mode */ }
  var TEXT = {
    nolink: { th: 'ลิงก์ไม่ครบ ลองเปิดจากอีเมลอีกครั้ง', en: 'The link is incomplete. Open it again from the email.' },
    invalid: { th: 'ลิงก์นี้ใช้ไม่ได้แล้ว ติดต่อทีมเพื่อขอลิงก์ใหม่', en: 'This link no longer works. Ask the team for a new one.' },
    not_ready: { th: 'ยังไม่มีไฟล์ให้ดาวน์โหลด ลองใหม่อีกครั้งภายหลัง', en: 'There is no file to download yet. Try again later.' },
    unavailable: { th: 'ระบบไม่ตอบ ลองใหม่อีกครั้ง', en: 'The service did not answer. Try again.' },
    starting: { th: 'กำลังเริ่มดาวน์โหลด…', en: 'Starting the download…' }
  };
  var shown = null;
  function say(k) { shown = k; msg.textContent = k ? TEXT[k][lang] : ''; }
  function setLang(l) {
    lang = l; root.dataset.lang = l; root.lang = l;
    try { localStorage.setItem('kohbor.lang', l); } catch (e) { /* private mode */ }
    Array.prototype.forEach.call(document.querySelectorAll('[data-set-lang]'), function (b) { b.setAttribute('aria-pressed', String(b.dataset.setLang === l)); });
    if (shown) say(shown);
  }
  Array.prototype.forEach.call(document.querySelectorAll('[data-set-lang]'), function (b) { b.addEventListener('click', function () { setLang(b.dataset.setLang); }); });
  setLang(lang);

  var link = location.hash.replace(/^#/, '');
  if (!/^[A-Za-z0-9_-]{32,64}$/.test(link)) { go.disabled = true; say('nolink'); return; }

  go.addEventListener('click', function () {
    go.disabled = true;
    say(null);
    fetch(ENDPOINT, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ link: link }),
      credentials: 'omit',
      referrerPolicy: 'no-referrer'
    }).then(function (res) {
      return res.json().catch(function () { return {}; }).then(function (body) {
        if (res.ok && body && typeof body.url === 'string' && body.url.indexOf('https://') === 0) {
          say('starting');
          window.location.assign(body.url);
          setTimeout(function () { go.disabled = false; }, 4000);
          return;
        }
        go.disabled = false;
        var err = body && body.error;
        say(err === 'invalid' ? 'invalid' : err === 'not_ready' ? 'not_ready' : 'unavailable');
      });
    }).catch(function () { go.disabled = false; say('unavailable'); });
  });
})();
