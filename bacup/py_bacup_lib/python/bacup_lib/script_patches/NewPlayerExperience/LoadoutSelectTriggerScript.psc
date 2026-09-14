Event OnTriggerEnter(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef == Game.GetPlayer() && NPE_LoadoutsEnabled != None && NPE_LoadoutsEnabled.GetValue() > 0.0
		playerRef.ShowSpecialBuildsMenu(GetReference())
		Quest owningQuest = GetOwningQuest()
		If owningQuest != None && SelectedLoadoutStage >= 0
			owningQuest.SetStage(SelectedLoadoutStage)
		EndIf
	EndIf
EndEvent
