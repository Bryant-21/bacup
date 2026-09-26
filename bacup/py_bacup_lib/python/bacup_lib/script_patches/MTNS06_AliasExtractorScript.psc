mtns06questscript Function EventScript()
	Quest owner = GetOwningQuest()
	Return owner as mtns06questscript
EndFunction

Bool Function ExtractorIntact()
	ObjectReference extractorRef = GetReference()
	Return extractorRef != None && !extractorRef.IsDisabled() && !DefaultAliasOnObjectRepaired.NeedsRepair(extractorRef)
EndFunction

Function StartExtracting()
	CancelTimer(TimerId)
	ExtractorIsOn = ExtractorIntact()
	If ExtractorIsOn
		GoToState("extractoron")
	Else
		GoToState("extractoroff")
	EndIf
	StartTimer(TimerLength as Float, TimerId)
EndFunction

Function StopExtracting()
	CancelTimer(TimerId)
	ExtractorIsOn = False
	GoToState("extractoroff")
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID != TimerId
		Return
	EndIf
	mtns06questscript eventScript = EventScript()
	If eventScript == None || !eventScript.bActivityIsActive || GetOwningQuest().IsStageDone(CooldownStage)
		StopExtracting()
		Return
	EndIf
	If ExtractorIsOn && ExtractorIntact()
		eventScript.AddExtractorUranium(TimerLength as Float)
	EndIf
	StartTimer(TimerLength as Float, TimerId)
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
	mtns06questscript eventScript = EventScript()
	If aiCurrentStage > 0
		If ExtractorIsOn
			ExtractorIsOn = False
			GoToState("extractoroff")
			If eventScript != None
				eventScript.ExtractorDestroyed(Self)
			EndIf
		EndIf
	ElseIf aiOldStage > 0
		If eventScript != None
			ExtractorIsOn = eventScript.bActivityIsActive
			If ExtractorIsOn
				GoToState("extractoron")
			EndIf
			eventScript.ExtractorRepaired(Self)
		EndIf
	EndIf
EndEvent

Event OnAliasShutdown()
	StopExtracting()
EndEvent
