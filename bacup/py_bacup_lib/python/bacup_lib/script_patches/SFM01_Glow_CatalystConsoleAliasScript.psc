Event OnActivate(ObjectReference akActionRef)
	Quest owningQuest = GetOwningQuest()
	Actor playerRef = OwningPlayerAlias.GetActorReference()
	If owningQuest == None || playerRef == None || akActionRef != playerRef || !owningQuest.IsRunning() || MessageToDisplay == None
		Return
	EndIf

	Int selectedButton = MessageToDisplay.Show()
	If selectedButton == 1 && CatalystToUse != None && StageToSet > 0 && !owningQuest.IsStageDone(StageToSet) && playerRef.GetItemCount(CatalystToUse) > 0
		playerRef.RemoveItem(CatalystToUse, 1, True)
		If ConsoleReference != None
			ConsoleReference.PlayAnimation("CatalystIn01")
		EndIf
		owningQuest.SetStage(StageToSet)
	EndIf
EndEvent
