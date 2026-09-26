; @drop-member TW002SecurityStationScript.tw002securitystationscript_TW002GotTape

Event OnQuestInit()
	RegisterForStationEvents()
EndEvent

Function RegisterForStationEvents()
	If !IsRunning() || IsCompleted()
		Return
	EndIf
	RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
	RegisterSecurityStation(Alias_SecurityStation01)
	RegisterSecurityStation(Alias_SecurityStation02)
	RegisterSecurityStation(Alias_SecurityStation03)
	RegisterSecurityStation(Alias_SecurityStation04)
EndFunction

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer()
		RegisterForStationEvents()
	EndIf
EndEvent

Event OnQuestShutdown()
	UnregisterForAllEvents()
EndEvent

Function RegisterSecurityStation(ReferenceAlias stationAlias)
	If stationAlias == None
		Return
	EndIf

	TW002SecurityStationScript station = stationAlias.GetReference() as TW002SecurityStationScript
	If station != None
		RegisterForCustomEvent(station, "TW002GotTape")
	EndIf
EndFunction

Event TW002SecurityStationScript.TW002GotTape(TW002SecurityStationScript akSender, Var[] akArgs)
	If !IsRunning() || IsCompleted() || !IsStageDone(QuestStartStage) || IsStageDone(GotTapesStage)
		Return
	EndIf
	ObjectReference stationRef = akSender as ObjectReference
	If stationRef == None
		Return
	EndIf

	If Alias_SecurityStation01 != None && stationRef == Alias_SecurityStation01.GetReference()
		If !IsStageDone(Station01Stage)
			SetStage(Station01Stage)
		EndIf
	ElseIf Alias_SecurityStation02 != None && stationRef == Alias_SecurityStation02.GetReference()
		If !IsStageDone(Station02Stage)
			SetStage(Station02Stage)
		EndIf
	ElseIf Alias_SecurityStation03 != None && stationRef == Alias_SecurityStation03.GetReference()
		If !IsStageDone(Station03Stage)
			SetStage(Station03Stage)
		EndIf
	ElseIf Alias_SecurityStation04 != None && stationRef == Alias_SecurityStation04.GetReference()
		If !IsStageDone(Station04Stage)
			SetStage(Station04Stage)
		EndIf
	EndIf
EndEvent
