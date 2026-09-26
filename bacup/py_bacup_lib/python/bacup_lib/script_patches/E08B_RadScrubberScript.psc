Event OnAliasInit()
	CancelTimer(repairGracePeriodTimerID)
	CancelTimer(radiationCooldownTimerID)
	isActivated = False
	ObjectReference scrubber = GetReference()
	If scrubber == None
		Return
	EndIf
	If scrubber.GetCurrentDestructionStage() > 0
		scrubber.ClearDestruction()
	EndIf
	RemoveSmoke(scrubber)
	; Until the defense stage opens, the scrubber takes no damage.
	If IgnoreDamageKeyword != None
		scrubber.AddKeyword(IgnoreDamageKeyword)
	EndIf
	SetMachinesRunning(False)
EndEvent

Event OnActivate(ObjectReference akActionRef)
	Quest owner = GetOwningQuest()
	If akActionRef != Game.GetPlayer() || owner == None || !owner.IsRunning() || IsEventOver(owner)
		Return
	EndIf
	If !isActivated
		If !owner.IsObjectiveDisplayed(ActivateObjective) || owner.IsObjectiveCompleted(ActivateObjective)
			Return
		EndIf
		isActivated = True
		owner.SetObjectiveCompleted(ActivateObjective, True)
		SetMachinesRunning(True)
		If PostActivateStage >= 0 && !owner.IsStageDone(PostActivateStage)
			owner.SetStage(PostActivateStage)
		EndIf
	ElseIf NeedsRepair()
		RepairScrubber(owner)
	EndIf
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
	; The scrubber's damage stage 1 caps its health at 1% and swaps in the hulk model: that is "broken".
	If aiOldStage == 0 && aiCurrentStage > 0
		BreakScrubber()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	Quest owner = GetOwningQuest()
	If owner == None || !owner.IsRunning() || IsEventOver(owner)
		Return
	EndIf
	If aiTimerID == radiationCooldownTimerID
		If !NeedsRepair()
			SetRadiationLevel(owner, 1)
		EndIf
	ElseIf aiTimerID == repairGracePeriodTimerID
		; B21:ObjectiveTimers sets stage 550 only once; this grace timer fails every later breakdown too.
		If NeedsRepair()
			owner.SetObjectiveFailed(RepairObjective, True)
			owner.SetStage(9999)
		EndIf
	EndIf
EndEvent

Event OnAliasShutdown()
	CancelTimer(repairGracePeriodTimerID)
	CancelTimer(radiationCooldownTimerID)
	isActivated = False
	ObjectReference scrubber = GetReference()
	If scrubber == None
		Return
	EndIf
	If scrubber.GetCurrentDestructionStage() > 0
		scrubber.ClearDestruction()
	EndIf
	RemoveSmoke(scrubber)
	If IgnoreDamageKeyword != None
		scrubber.RemoveKeyword(IgnoreDamageKeyword)
	EndIf
	SetMachinesRunning(False)
EndEvent

Bool Function IsEventOver(Quest akOwner)
	Return akOwner.IsStageDone(9000) || akOwner.IsStageDone(9998) || akOwner.IsStageDone(9999)
EndFunction

Bool Function NeedsRepair()
	ObjectReference scrubber = GetReference()
	Return scrubber != None && scrubber.GetCurrentDestructionStage() > 0
EndFunction

Function SetRadiationLevel(Quest akOwner, Int aiLevel)
	E08B_EvictionNoticeScript eventScript = akOwner as E08B_EvictionNoticeScript
	If eventScript != None
		eventScript.SetRadiationLevel(aiLevel)
	EndIf
EndFunction

Function SetMachinesRunning(Bool abRunning)
	ObjectReference scrubber = GetReference()
	GeneratorAnims = scrubber as E08B_GeneratorAnimationScript
	If GeneratorAnims != None
		If abRunning
			GeneratorAnims.MoveStateOnClients(GeneratorAnims.OnAnimName, True)
		Else
			GeneratorAnims.MoveStateOnClients(GeneratorAnims.OffAnimName, False)
		EndIf
	EndIf
	WindmillAnims = None
	If WindmillAlias != None
		WindmillAnims = WindmillAlias.GetReference() as E08B_WindmillAnimScript
	EndIf
	If WindmillAnims != None
		If abRunning
			WindmillAnims.SetState(2)
		Else
			WindmillAnims.SetState(0)
		EndIf
	EndIf
EndFunction

Function RemoveSmoke(ObjectReference akScrubber)
	If SmokeFX == None
		Return
	EndIf
	ObjectReference smoke = Game.FindClosestReferenceOfTypeFromRef(SmokeFX, akScrubber, 256.0)
	If smoke != None
		smoke.Disable(False)
		smoke.Delete()
	EndIf
EndFunction

Function BreakScrubber()
	Quest owner = GetOwningQuest()
	ObjectReference scrubber = GetReference()
	If owner == None || scrubber == None || !owner.IsRunning() || IsEventOver(owner)
		Return
	EndIf
	If IgnoreDamageKeyword != None
		scrubber.AddKeyword(IgnoreDamageKeyword)
	EndIf
	CancelTimer(radiationCooldownTimerID)
	SetMachinesRunning(False)
	If SmokeFX != None
		scrubber.PlaceAtMe(SmokeFX, 1, False, False, True)
	EndIf
	If RepairMessage != None
		RepairMessage.Show()
	EndIf
	owner.SetObjectiveDisplayed(DefendObjective, False)
	owner.SetObjectiveCompleted(RepairObjective, False)
	owner.SetObjectiveFailed(RepairObjective, False)
	owner.SetObjectiveDisplayed(RepairObjective, True, True)
	SetRadiationLevel(owner, 2)

	Float repairSeconds = 30.0
	If RepairTimer != None && RepairTimer.GetValue() > 0.0
		repairSeconds = RepairTimer.GetValue()
	EndIf
	; One extra second covers the objective timer's one-second ticks, so the first breakdown fails through stage 550.
	StartTimer(repairSeconds + 1.0, repairGracePeriodTimerID)
EndFunction

Function RepairScrubber(Quest akOwner)
	ObjectReference scrubber = GetReference()
	If scrubber == None
		Return
	EndIf
	CancelTimer(repairGracePeriodTimerID)
	scrubber.ClearDestruction()
	RemoveSmoke(scrubber)
	If IgnoreDamageKeyword != None && akOwner.IsStageDone(250)
		scrubber.RemoveKeyword(IgnoreDamageKeyword)
	EndIf
	SetMachinesRunning(True)
	If RepairedMessage != None
		RepairedMessage.Show()
	EndIf
	; RepairedMessage: "5 Seconds until radiation level lowers."
	StartTimer(5.0, radiationCooldownTimerID)
	akOwner.SetStage(555)
EndFunction
