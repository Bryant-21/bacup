; OnEnterFurniture was a FO76 furniture-occupancy event with no Fallout 4 equivalent
; (FO4 raises OnSit/OnGetUp on the *actor*, not on the furniture reference). Its body
; -- starting the sit timer -- is re-homed into OnActivate, which is the same moment
; the sitter is committed.
; @drop-member OnEnterFurniture

Event OnActivate(ObjectReference akActivatorRef)
	If !Self.IsFurnitureInUse(False)
		; FO76's "is player" type test -> single-player identity test.
		If akActivatorRef == Game.GetPlayer() && akActivatorRef.HasKeyword(EN04_PlayerArmedInjector)
			SitterRef = akActivatorRef
			Self.Activate(akActivatorRef, False)
			Self.StartTimer(iSitTimerLength as Float, iSitTimerID)
		EndIf
	EndIf
EndEvent
