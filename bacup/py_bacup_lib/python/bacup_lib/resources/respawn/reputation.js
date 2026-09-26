(function () {
  'use strict';
  window.B21TalesReputation = function (raw) {
    var data; try { data = typeof raw === 'string' ? JSON.parse(raw) : raw; } catch (_) { return; }
    if (!data || data.version !== 1 || !Array.isArray(data.factions)) return;
    var host = document.getElementById('b21Reputation');
    if (!host) return;
    host.textContent = '';
    data.factions.forEach(function (faction) {
      if (['Crater', 'Foundation'].indexOf(faction.code) < 0 || !Number.isFinite(faction.progress)) return;
      var row = document.createElement('div'); row.className = 'b21-rep-row';
      var name = document.createElement('strong'); name.textContent = faction.name; row.appendChild(name);
      var tier = document.createElement('span'); tier.textContent = faction.tier; row.appendChild(tier);
      var meter = document.createElement('progress'); meter.max = 1; meter.value = Math.max(0, Math.min(1, faction.progress));
      meter.setAttribute('aria-label', faction.name + ' ' + faction.tier); row.appendChild(meter);
      var detail = document.createElement('span');
      var labels = data.labels || {};
      detail.textContent = faction.maximum ? labels.$B21_RespawnMaximum || 'Maximum reputation' :
        String(Math.max(0, Math.ceil(faction.remaining))) + ' ' + (labels.$B21_RespawnNextTier || 'to next tier');
      row.appendChild(detail); host.appendChild(row);
    });
    host.hidden = host.children.length === 0;
  };
}());
