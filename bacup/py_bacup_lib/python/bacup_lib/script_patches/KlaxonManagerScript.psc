; Re-homed from OnSyncVariableNetworkChanged: FO76 replicated KlaxonState and each
; client re-broadcast it as a custom event. Single-player has no replication, so the
; dispatch is exposed as a local function for whatever sets KlaxonState to call.
Function NotifyKlaxonStateChanged()
	While lock_KlaxonManagerState
		utility.Wait(0.1)
	EndWhile
	lock_KlaxonManagerState = True
	Var[] akArgs = new Var[1]
	akArgs[0] = KlaxonState as Var
	Self.SendCustomEvent("KlaxonManagerStateChangedClient", akArgs)
	lock_KlaxonManagerState = False
EndFunction

; @drop-member OnSyncVariableNetworkChanged
