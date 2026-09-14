Event OnQuestInit()
	RegisterSecurityStation(Alias_SecurityStation01)
	RegisterSecurityStation(Alias_SecurityStation02)
	RegisterSecurityStation(Alias_SecurityStation03)
	RegisterSecurityStation(Alias_SecurityStation04)
EndEvent

Function RegisterSecurityStation(ReferenceAlias stationAlias)
	If stationAlias == None
		Return
	EndIf

	TW002SecurityStationScript station = stationAlias.GetReference() as TW002SecurityStationScript
	If station != None
		RegisterForCustomEvent(station, "tw002securitystationscript_TW002GotTape")
	EndIf
EndFunction

Event TW002SecurityStationScript.tw002securitystationscript_TW002GotTape(TW002SecurityStationScript akSender, Var[] akArgs)
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
