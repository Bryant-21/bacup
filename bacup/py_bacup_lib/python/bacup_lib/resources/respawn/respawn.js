(function () {
  'use strict';
  var version = 1, sequence = 0, choices = [], pack = null, world = 0, active = false, sent = false;
  var root = document.getElementById('respawn'), image = document.getElementById('mapImage');
  var list = document.getElementById('locations'), markers = document.getElementById('markers');
  function read(raw) { try { return typeof raw === 'string' ? JSON.parse(raw) : raw; } catch (_) { return null; } }
  function send(operation, extra) {
    if (typeof window.b21RespawnEvent !== 'function') return;
    var data = {version:version, sequence:sequence, operation:operation};
    Object.keys(extra || {}).forEach(function (key) { data[key] = extra[key]; });
    window.b21RespawnEvent(JSON.stringify(data));
  }
  function select(id) {
    if (!active || sent || !choices.some(function (point) { return point.id === id; })) return;
    sent = true; send('select', {id:id});
  }
  function pixel(point) {
    var c = pack && pack.calibration;
    if (!c) return null;
    if (c.mode === 'survey' && c.maxRange > 0 && c.mapPx > 0)
      return [(point.x-c.centerX)*c.mapPx/c.maxRange+c.mapPx/2, (c.centerY-point.y)*c.mapPx/c.maxRange+c.mapPx/2];
    if (c.mode === 'frame') {
      var width = (c.seCellX + 1 - c.nwCellX)*4096, height = (c.nwCellY + 1 - c.seCellY)*4096;
      if (width <= 0 || height <= 0) return null;
      return [c.x0+(point.x-c.nwCellX*4096)*(c.x1-c.x0)/width,
        c.y0+((c.nwCellY+1)*4096-point.y)*(c.y1-c.y0)/height];
    }
    return null;
  }
  function drawMarkers() {
    markers.textContent = '';
    var width = image.naturalWidth, height = image.naturalHeight;
    if (!width || !height) return;
    var scale = Math.min(markers.clientWidth/width, markers.clientHeight/height);
    var left = (markers.clientWidth-width*scale)/2, top = (markers.clientHeight-height*scale)/2;
    choices.forEach(function (point) {
      var p = pixel(point); if (!p || !p.every(Number.isFinite) || p[0]<0 || p[1]<0 || p[0]>width || p[1]>height) return;
      var button = document.createElement('button'); button.className = 'map-choice';
      button.title = point.name; button.setAttribute('aria-label', point.name);
      button.style.left = (left+p[0]*scale)+'px'; button.style.top = (top+p[1]*scale)+'px';
      button.onclick = function () { select(point.id); }; markers.appendChild(button);
    });
  }
  window.B21RespawnHello = function (raw) { var data = read(raw); if (data && data.version === version) send('ready'); };
  window.B21RespawnMap = function (raw) {
    var data = read(raw); if (!data || data.version !== version || active) return;
    pack = data.pack; world = data.world;
    if (!pack || typeof pack.image !== 'string' || pack.image.indexOf('../maps/') !== 0) return;
    var requestedWorld = world;
    image.onload = function () { send('image', {world:requestedWorld, ready:true}); drawMarkers(); };
    image.onerror = function () { send('image', {world:requestedWorld, ready:false}); };
    image.src = pack.image;
  };
  window.B21RespawnChoices = function (raw) {
    var data = read(raw);
    if (!data || data.version !== version || !Number.isInteger(data.sequence) || data.sequence <= 0 || !Array.isArray(data.choices)) return;
    sequence = data.sequence; choices = data.choices.filter(function (point) { return Number.isInteger(point.id) && point.id > 0 && Number.isFinite(point.x) && Number.isFinite(point.y); });
    var labels = data.labels || {};
    document.getElementById('heading').textContent = labels.$B21_RespawnTitle || 'Respawn';
    document.getElementById('cancel').textContent = labels.$B21_RespawnNearestSafe || 'Use nearest safe location';
    active = true; sent = false; root.hidden = false; list.textContent = '';
    document.getElementById('notice').textContent = labels.$B21_RespawnBagNotice || 'Your dropped junk remains at the death site.';
    choices.forEach(function (point) {
      var button = document.createElement('button'); button.textContent = point.name;
      button.onclick = function () { select(point.id); }; list.appendChild(button);
    });
    requestAnimationFrame(drawMarkers); if (list.firstChild) list.firstChild.focus();
  };
  window.B21RespawnComplete = function (raw) {
    var data = read(raw); if (!data || data.version !== version || data.sequence !== sequence) return;
    active = false; choices = []; root.hidden = true; list.textContent = ''; markers.textContent = '';
  };
  window.B21RespawnNotice = function (notice) { sent = false; document.getElementById('notice').textContent = String(notice); };
  document.getElementById('cancel').onclick = function () { if (active) send('cancel'); };
  document.addEventListener('keydown', function (event) {
    if (!active) return;
    if (event.key === 'Escape') { event.preventDefault(); send('cancel'); }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      var buttons = Array.prototype.slice.call(list.children); buttons.push(document.getElementById('cancel'));
      var index = buttons.indexOf(document.activeElement);
      buttons[(index+(event.key === 'ArrowDown' ? 1 : buttons.length-1))%buttons.length].focus();
    }
  });
  window.addEventListener('resize', drawMarkers);
}());
