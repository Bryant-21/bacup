; Disable() never sends OnTriggerLeave. While the player carries the spell a 1 s check releases it
; once every trigger in the collection is off (floors, telegraphs and the platform switch as a
; group), and the quest script also calls ReleaseOccupants() whenever it turns them off. Only the
; player is affected: the Guardian stands on the platform-surface trigger (2400 damage per second).
Event OnAliasInit()
	Int index = 0
	While index < GetCount()
		ObjectReference triggerRef = GetAt(index)
		If triggerRef != None
			RegisterForRemoteEvent(triggerRef, "OnTriggerEnter")
			RegisterForRemoteEvent(triggerRef, "OnTriggerLeave")
		EndIf
		index += 1
	EndWhile
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	Actor player = Game.GetPlayer()
	If akActionRef == player && SpellToApply != None && !akSender.IsDisabled()
		player.AddSpell(SpellToApply, False)
		StartTimer(1.0, 1)
	EndIf
EndEvent

Event ObjectReference.OnTriggerLeave(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && !OtherTriggerOccupied(akSender)
		ReleaseOccupants()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != 1
		Return
	EndIf
	If AnyTriggerEnabled()
		StartTimer(1.0, 1)
	Else
		ReleaseOccupants()
	EndIf
EndEvent

Bool Function AnyTriggerEnabled()
	Int index = 0
	While index < GetCount()
		ObjectReference triggerRef = GetAt(index)
		If triggerRef != None && !triggerRef.IsDisabled()
			Return True
		EndIf
		index += 1
	EndWhile
	Return False
EndFunction

; Stepping from one live floor straight onto another can deliver the leave after the enter.
Bool Function OtherTriggerOccupied(ObjectReference akLeftTrigger)
	Int index = 0
	While index < GetCount()
		ObjectReference triggerRef = GetAt(index)
		If triggerRef != None && triggerRef != akLeftTrigger && !triggerRef.IsDisabled() && triggerRef.GetTriggerObjectCount() > 0
			Return True
		EndIf
		index += 1
	EndWhile
	Return False
EndFunction

Function ReleaseOccupants()
	CancelTimer(1)
	If SpellToApply != None
		Game.GetPlayer().RemoveSpell(SpellToApply)
	EndIf
EndFunction

Function SetTriggersEnabled(Bool abEnabled)
	Int index = 0
	While index < GetCount()
		ObjectReference triggerRef = GetAt(index)
		If triggerRef != None
			If abEnabled
				triggerRef.Enable()
			Else
				triggerRef.Disable()
			EndIf
		EndIf
		index += 1
	EndWhile
	If !abEnabled
		ReleaseOccupants()
	EndIf
EndFunction
