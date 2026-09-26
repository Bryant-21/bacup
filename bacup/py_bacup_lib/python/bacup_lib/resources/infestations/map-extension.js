(function () {
  'use strict';
  if (window.B21TalesInfestations) return;
  var world = document.getElementById('world');
  if (!world || typeof window.setWorld !== 'function') return;
  var version = 1, session = 0, shown = '', last = null;
  var host = document.createElement('div');
  host.id = 'b21Infestations';
  host.setAttribute('aria-hidden', 'true');
  host.style.cssText = 'position:absolute;left:0;top:0;width:0;height:0;pointer-events:none';
  var tint = document.getElementById('mapTint');
  world.insertBefore(host, tint ? tint.nextSibling : null);
  var sheet = document.createElement('style');
  sheet.textContent = '@keyframes b21InfestationSpin{from{transform:rotate(0deg)}to{transform:rotate(360deg)}}' +
    '#b21Infestations .b21-fog{position:absolute}' +
    '#b21Infestations .b21-fog img{position:absolute;left:50%;top:50%;animation:b21InfestationSpin linear infinite}' +
    '#b21Infestations .b21-site{position:absolute;width:18px;height:18px;margin:-9px 0 0 -9px;border:3px solid #d7d0c0;' +
    'border-radius:50%;background:rgba(20,18,16,.85);box-shadow:0 0 0 2px rgba(20,18,16,.9)}';
  document.head.appendChild(sheet);

  function read(raw) { try { return typeof raw === 'string' ? JSON.parse(raw) : raw; } catch (_) { return null; } }
  function finite() { for (var i = 0; i < arguments.length; i++) if (typeof arguments[i] !== 'number' || !isFinite(arguments[i])) return false; return true; }
  function render() {
    host.textContent = '';
    if (!last || String(last.world).toLowerCase() !== shown.toLowerCase()) return;
    last.fogs.forEach(function (fog) {
      if (!finite(fog.px, fog.py, fog.r) || fog.r <= 0) return;
      var box = document.createElement('div');
      box.className = 'b21-fog';
      box.style.left = fog.px + 'px';
      box.style.top = fog.py + 'px';
      last.layers.forEach(function (layer, index) {
        if (!finite(layer.scale, layer.period) || layer.scale <= 0) return;
        var size = 2 * fog.r * layer.scale, image = new Image();
        image.src = layer.src;
        image.alt = '';
        image.style.width = image.style.height = size + 'px';
        image.style.marginLeft = image.style.marginTop = (-size / 2) + 'px';
        image.style.animationDuration = Math.max(1, layer.period) + 's';
        image.style.animationDirection = index % 2 ? 'reverse' : 'normal';
        image.style.opacity = fog.discovered ? '0.6' : '0.95';
        box.appendChild(image);
      });
      host.appendChild(box);
      if (fog.discovered && finite(fog.sx, fog.sy)) {
        var site = document.createElement('div');
        site.className = 'b21-site';
        site.title = fog.name || '';
        site.style.left = fog.sx + 'px';
        site.style.top = fog.sy + 'px';
        host.appendChild(site);
      }
    });
  }
  var setWorld = window.setWorld;
  window.setWorld = function (raw) {
    var result = setWorld.apply(this, arguments), data = read(raw);
    shown = data && (data.worldspace || data.id) || '';
    render();
    return result;
  };
  window.B21InfestationSession = function (raw) {
    var data = read(raw);
    if (!data || data.version !== version) return;
    if (data.session !== session) { session = data.session; last = null; render(); }
    if (typeof window.b21InfestationMapEvent === 'function')
      window.b21InfestationMapEvent(JSON.stringify({version: version, session: session, operation: 'ready'}));
  };
  window.B21TalesInfestations = function (raw) {
    var data = read(raw);
    if (!data || data.version !== version || data.session !== session || !Array.isArray(data.fogs) || !Array.isArray(data.layers)) return;
    last = data;
    render();
  };
}());
