// Alpha download gate: the invite code is checked by a Supabase Edge Function,
// which answers with a short-lived link to the installer. No cookies, no tracking.
(function () {
  'use strict';
  var ENDPOINT = 'https://wmmvawvfnwdfzxvumlfc.supabase.co/functions/v1/alpha-download';
  var open = document.getElementById('dlOpen');
  var dialog = document.getElementById('dlDialog');
  var form = document.getElementById('dlForm');
  var input = document.getElementById('dlCode');
  var msg = document.getElementById('dlMsg');
  var go = document.getElementById('dlGo');
  var cancel = document.getElementById('dlCancel');
  if (!open || !dialog || typeof dialog.showModal !== 'function') return;

  var TEXT = {
    empty: 'ใส่ invite code ก่อน · Enter your invite code first.',
    checking: 'กำลังตรวจโค้ด… · Checking the code…',
    invalid: 'โค้ดนี้ใช้ไม่ได้ หรือใช้ครบจำนวนแล้ว · This code is not valid or has been used up.',
    not_ready: 'โค้ดถูกต้อง แต่ยังไม่เปิดให้ดาวน์โหลด ลองใหม่อีกครั้งภายหลัง · Your code is fine, but the download is not open yet. Try again later.',
    unavailable: 'ระบบไม่ตอบ ลองใหม่อีกครั้ง · The service did not answer. Try again.',
    starting: 'กำลังเริ่มดาวน์โหลด… · Starting the download…'
  };

  function say(key) { msg.textContent = TEXT[key]; }

  open.addEventListener('click', function () {
    msg.textContent = '';
    input.value = '';
    dialog.showModal();
    input.focus();
  });
  cancel.addEventListener('click', function () { dialog.close(); open.focus(); });
  dialog.addEventListener('close', function () { go.disabled = false; });

  form.addEventListener('submit', function (event) {
    event.preventDefault();
    var code = input.value.trim();
    if (!code) { say('empty'); input.focus(); return; }
    go.disabled = true;
    say('checking');
    fetch(ENDPOINT, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ code: code }),
      credentials: 'omit',
      referrerPolicy: 'no-referrer'
    }).then(function (res) {
      return res.json().catch(function () { return {}; }).then(function (body) {
        if (res.ok && body && typeof body.url === 'string' && body.url.indexOf('https://') === 0) {
          say('starting');
          window.location.assign(body.url);
          return;
        }
        go.disabled = false;
        var err = body && body.error;
        say(err === 'invalid' ? 'invalid' : err === 'not_ready' ? 'not_ready' : 'unavailable');
      });
    }).catch(function () { go.disabled = false; say('unavailable'); });
  });
})();
