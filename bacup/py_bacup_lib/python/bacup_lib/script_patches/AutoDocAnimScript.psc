; OnEnterFurniture is a FO76 furniture-occupancy event Fallout 4 never raises (FO4
; raises OnSit on the actor, not on the furniture reference), and FO76's
; ObjectReference.GetFurnitureUsers(bool) has no FO4 counterpart at all -- the
; occupancy roster cannot be enumerated. The body is re-homed onto the actor's own
; OnSit, which makes the "is the player the sitter" test exact instead of a roster
; search.
; @drop-member OnEnterFurniture

Event OnLoad()
	Self.RegisterForRemoteEvent(Game.GetPlayer(), "OnSit")
EndEvent

Event actor.OnSit(actor akSender, ObjectReference akFurniture)
	If akFurniture != Self
		Return
	EndIf
	Self.PlayAnimationAndWait("furnitureidle", "FurnitureIdleStop")
	Self.Activate(akSender as ObjectReference, False)
EndEvent
