; Debug visualiser for the Encounter/Wave Spawn system. FO76's
; ObjectReference.GetRefIDString() (hex FormID as a string) has no FO4 equivalent, so
; the ref is formatted with Papyrus' own object-to-string conversion instead, which
; already renders as "[ScriptName <EditorID (FormID)>]". The debug strings the FO76
; version built were only ever fed to server-side RMI channels that do not exist here,
; so they are now traced locally.

Function cRMIStartSpawningDebug(ObjectReference SpawnAreaRef)
	If SpawnAreaRef == None
		Return
	EndIf
	Int foundIndex = TrackedSpawnAreas.Find(SpawnAreaRef)
	If foundIndex < 0
		TrackedSpawnAreas.Add(SpawnAreaRef)
		debug.Trace(SpawnAreaRef as String + ".StartSpawnDebug", 0)
	Else
		debug.Trace(SpawnAreaRef as String + ".ClearSpawnDebug", 0)
	EndIf
EndFunction

Function cRMIStopSpawningDebug(ObjectReference SpawnAreaRef)
	If SpawnAreaRef == None
		Return
	EndIf
	Int foundIndex = TrackedSpawnAreas.Find(SpawnAreaRef)
	If foundIndex >= 0
		TrackedSpawnAreas.Remove(foundIndex)
	EndIf
	debug.Trace(SpawnAreaRef as String + ".StopSpawnDebug", 0)
EndFunction

Event OnInit()
	TrackedSpawnAreas = new objectreference[0]
	LocalPlayers = new Actor[0]
EndEvent

Event OnUnload()
	Int foundIndex = LocalPlayers.Find(Game.GetPlayer())
	If foundIndex >= 0
		LocalPlayers.Remove(foundIndex)
	EndIf
EndEvent
