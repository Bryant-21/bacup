(function () {
  'use strict';
  if (window.B21TalesMapSession) return;
  if (!document.body || !window.setWorld || !window.setDiscovery) return;
  var world = '', session = 0;
  var host = document.createElement('aside'); host.id = 'b21Reputation'; host.hidden = true;
  host.setAttribute('aria-label', 'Faction reputation');
  host.style.cssText = 'position:fixed;right:22px;bottom:90px;width:280px;padding:16px;background:rgba(15,24,24,.94);color:#f3e8c8;z-index:20;pointer-events:none';
  document.body.appendChild(host);
  var sheet = document.createElement('style');
  sheet.textContent = '#b21Reputation .b21-rep-row+div{margin-top:14px}#b21Reputation strong,#b21Reputation span{display:block;margin:4px 0}#b21Reputation progress{width:100%;height:12px}';
  document.head.appendChild(sheet);
  var script = document.createElement('script'); script.src = 'tales/reputation.js'; document.head.appendChild(script);
  window.B21TalesMapSession = function (raw) {
    var data; try { data = JSON.parse(raw); } catch (_) { return; }
    if (data.version !== 1) return;
    session = data.session; world = ''; host.hidden = true;
  };
  function wrap(name, capture) {
    var original = window[name];
    if (typeof original !== 'function') return;
    window[name] = function (raw) {
      var result = original.apply(this, arguments);
      try { capture(typeof raw === 'string' ? JSON.parse(raw) : raw); } catch (_) { /* Native refresh retries. */ }
      return result;
    };
  }
  wrap('setWorld', function (data) { world = data.worldspace || ''; });
  wrap('setDiscovery', function (states) {
    if (typeof window.b21TalesMapEvent === 'function' && world && Array.isArray(states))
      window.b21TalesMapEvent(JSON.stringify({version:1, session:session, world:world, states:states}));
  });
  if (typeof window.b21TalesMapEvent === 'function')
    window.b21TalesMapEvent(JSON.stringify({version:1, session:session, operation:'ready'}));
}());
