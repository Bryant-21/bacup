Event OnAliasInit()
	EnsureRepairTargetRegistration()
EndEvent

Event OnLoad()
	EnsureRepairTargetRegistration()
EndEvent

Function EnsureRepairTargetRegistration()
	ObjectReference repairTarget = RefToBreak.GetReference()
	If repairTarget != None
		UnregisterForRemoteEvent(repairTarget, "OnActivate")
		RegisterForRemoteEvent(repairTarget, "OnActivate")
	EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	Quest owningQuest = GetOwningQuest()
	If akActionRef != Game.GetPlayer() || owningQuest == None || akSender != RefToBreak.GetReference()
		Return
	EndIf

	If owningQuest.IsRunning() && owningQuest.IsStageDone(200) && !owningQuest.IsStageDone(StageToCheck)
		owningQuest.SetStage(StageToCheck)
	EndIf
EndEvent
