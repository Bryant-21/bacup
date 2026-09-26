Event OnActivate(ObjectReference akActivator)
	Quest owningQuest = GetOwningQuest()
	If owningQuest == None
		Return
	EndIf

	; The Player alias is filled from the story event in FO76; when that fill is unavailable
	; the live player reference is still the only activator this feedback applies to.
	ObjectReference playerRef = None
	If Alias_Player != None
		playerRef = Alias_Player.GetReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If akActivator != playerRef
		Return
	EndIf

	If owningQuest.IsStageDone(StageToCheck)
		TWZ03_AlreadyPlaced.Show()
	ElseIf !owningQuest.IsStageDone(HasTargetsStage)
		TWZ03_NoTargets.Show()
	EndIf
EndEvent
