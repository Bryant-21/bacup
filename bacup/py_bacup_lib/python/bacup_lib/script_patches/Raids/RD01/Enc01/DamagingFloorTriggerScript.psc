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
	Actor enteringActor = akActionRef as Actor
	If enteringActor != None && SpellToApply != None
		enteringActor.AddSpell(SpellToApply, False)
	EndIf
EndEvent

Event ObjectReference.OnTriggerLeave(ObjectReference akSender, ObjectReference akActionRef)
	Actor leavingActor = akActionRef as Actor
	If leavingActor != None && SpellToApply != None
		leavingActor.RemoveSpell(SpellToApply)
	EndIf
EndEvent
