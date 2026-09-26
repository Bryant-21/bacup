; Occupancy only. The quest script decides at blast time and applies the kill imagespace to a
; player who is NOT sheltered; entering a shelter must never apply it.
Event OnTriggerEnter(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		IsActive = True
	EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		IsActive = False
	EndIf
EndEvent

Function TurnOn()
	IsActive = False
	ObjectReference triggerRef = GetReference()
	If triggerRef != None
		triggerRef.Enable()
	EndIf
EndFunction

Function TurnOff()
	IsActive = False
	ObjectReference triggerRef = GetReference()
	If triggerRef != None
		triggerRef.Disable()
	EndIf
EndFunction

Bool Function IsPlayerSheltered()
	ObjectReference triggerRef = GetReference()
	If triggerRef == None || triggerRef.IsDisabled()
		Return False
	EndIf
	Return IsActive
EndFunction
