Bool Function EnterShelter(Int aiFormId)
	form ShelterEntranceForm = game.GetForm(aiFormId)
	If ShelterEntranceForm == None
		Return False
	EndIf
	; FO76 Actor.FindClosestValidReferenceOfType(form, radius) -> FO4
	; Game.FindClosestReferenceOfTypeFromRef(form, center, radius). FO76's extra
	; "valid" filtering (loaded/enabled/unreserved) has no FO4 counterpart.
	objectreference activeDoor = game.FindClosestReferenceOfTypeFromRef(ShelterEntranceForm, Self as objectreference, 5000.0)
	If activeDoor != None
		activeDoor.Activate(Self as objectreference, False)
		Return True
	EndIf
	Return False
EndFunction
