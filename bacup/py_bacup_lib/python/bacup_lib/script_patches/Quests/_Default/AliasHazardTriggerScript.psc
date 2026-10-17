Event OnAliasInit()
	OwningQuest = Self.GetOwningQuest()
	SelfRef = Self.GetReference()
	TriggerSpellUse = TriggerSpell
EndEvent

Bool Function HazardActive()
	If !OwningQuest
		OwningQuest = Self.GetOwningQuest()
	EndIf
	If !OwningQuest || !OwningQuest.IsRunning()
		Return False
	EndIf
	If PrereqStage >= 0 && !OwningQuest.IsStageDone(PrereqStage)
		Return False
	EndIf
	If TurnOffStage >= 0 && OwningQuest.IsStageDone(TurnOffStage)
		Return False
	EndIf
	Return True
EndFunction

Event OnTriggerEnter(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() as ObjectReference && HazardActive()
		Self.StartTimer(TriggerSpellDelay, CONST_TriggerSpellTimerID)
	EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() as ObjectReference
		Self.CancelTimer(CONST_TriggerSpellTimerID)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != CONST_TriggerSpellTimerID
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	SelfRef = Self.GetReference()
	If !SelfRef || !TriggerSpell || !HazardActive() || SelfRef.GetTriggerObjectCount() <= 0
		Return
	EndIf
	TriggerSpell.Cast(SelfRef, playerRef as ObjectReference)
	Self.StartTimer(TriggerSpellDelay, CONST_TriggerSpellTimerID)
EndEvent
