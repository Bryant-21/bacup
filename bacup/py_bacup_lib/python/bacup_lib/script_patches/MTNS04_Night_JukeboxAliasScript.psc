Bool Function ObjectivesAllowed()
	If OwningQuest == None
		OwningQuest = GetOwningQuest()
	EndIf
	If OwningQuest == None || !OwningQuest.IsRunning()
		Return False
	EndIf
	If PrereqStage >= 0 && !OwningQuest.IsStageDone(PrereqStage)
		Return False
	EndIf
	Return TurnOffStage < 0 || !OwningQuest.IsStageDone(TurnOffStage)
EndFunction

Function UpdateJukeboxObjectives()
	If OwningQuest == None
		OwningQuest = GetOwningQuest()
	EndIf
	SelfRef = GetReference()
	JukeboxAnimScript = SelfRef as DefaultDestructible2StateActivator
	If !ObjectivesAllowed() || SelfRef == None
		If OwningQuest != None
			OwningQuest.SetObjectiveDisplayed(RepairObjective, False)
			OwningQuest.SetObjectiveDisplayed(TurnOnJukeboxObjective, False)
		EndIf
		Return
	EndIf

	If SelfRef.IsDestroyed()
		OwningQuest.SetObjectiveDisplayed(TurnOnJukeboxObjective, False)
		If !OwningQuest.IsObjectiveDisplayed(RepairObjective)
			OwningQuest.SetObjectiveDisplayed(RepairObjective, True, True)
		EndIf
	Else
		OwningQuest.SetObjectiveDisplayed(RepairObjective, False)
		Bool playing = JukeboxAnimScript != None && JukeboxAnimScript.IsOpen
		If playing
			OwningQuest.SetObjectiveDisplayed(TurnOnJukeboxObjective, False)
		ElseIf !OwningQuest.IsObjectiveDisplayed(TurnOnJukeboxObjective)
			OwningQuest.SetObjectiveDisplayed(TurnOnJukeboxObjective, True, True)
		EndIf
	EndIf
	StartTimer(3.0, 1)
EndFunction

Event OnAliasInit()
	UpdateJukeboxObjectives()
EndEvent

Event OnActivate(ObjectReference akActionRef)
	; The two-state activator finishes its open/close animation after activation.
	StartTimer(2.0, 1)
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
	UpdateJukeboxObjectives()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 1
		UpdateJukeboxObjectives()
	EndIf
EndEvent

Event OnAliasShutdown()
	CancelTimer(1)
EndEvent
