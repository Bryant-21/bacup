MTNS04QuestScript Function EventScript()
	If myQIScript == None
		myQI = GetOwningQuest()
		myQIScript = myQI as MTNS04QuestScript
	EndIf
	Return myQIScript
EndFunction

Bool Function PlayerIsPlaying(Actor akPlayer)
	ObjectReference instrument = GetReference()
	If akPlayer == None || instrument == None
		Return False
	EndIf
	; Stock FO4 Papyrus cannot name the actor's furniture, so require a seated player at an occupied instrument.
	Return akPlayer.GetSitState() >= 2 && instrument.IsFurnitureInUse(True) && akPlayer.GetDistance(instrument) < 256.0
EndFunction

Function StopPlaying()
	CancelTimer(InstrumentAggroTimerId)
	Actor playerRef = Game.GetPlayer()
	If PlayersPlayingInstruments != None && playerRef != None && PlayersPlayingInstruments.Find(playerRef) >= 0
		PlayersPlayingInstruments.RemoveRef(playerRef)
	EndIf
	ObjectReference instrument = GetReference()
	If FurnitureInUse != None && instrument != None && FurnitureInUse.Find(instrument) >= 0
		FurnitureInUse.RemoveRef(instrument)
	EndIf
EndFunction

Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	MTNS04QuestScript eventScript = EventScript()
	If akActionRef != playerRef || eventScript == None || !eventScript.IsAggroActive()
		Return
	EndIf
	If PlayersPlayingInstruments != None && PlayersPlayingInstruments.Find(playerRef) < 0
		PlayersPlayingInstruments.AddRef(playerRef)
	EndIf
	ObjectReference instrument = GetReference()
	If FurnitureInUse != None && instrument != None && FurnitureInUse.Find(instrument) < 0
		FurnitureInUse.AddRef(instrument)
	EndIf
	eventScript.AddInstrumentAggro(playerRef)
	StartTimer(2.0, InstrumentAggroTimerId)
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != InstrumentAggroTimerId
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	MTNS04QuestScript eventScript = EventScript()
	If eventScript == None || !eventScript.IsAggroActive() || !PlayerIsPlaying(playerRef)
		StopPlaying()
		Return
	EndIf
	eventScript.AddInstrumentAggro(playerRef)
	StartTimer(2.0, InstrumentAggroTimerId)
EndEvent

Event OnAliasShutdown()
	StopPlaying()
EndEvent
