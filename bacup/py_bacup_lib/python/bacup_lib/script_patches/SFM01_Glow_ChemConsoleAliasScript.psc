Event OnActivate(ObjectReference akActionRef)
	Quest owningQuest = GetOwningQuest()
	Actor playerRef = OwningPlayerAlias.GetActorReference()
	If owningQuest == None || playerRef == None || akActionRef != playerRef || !owningQuest.IsRunning() || MessageToDisplay == None
		Return
	EndIf

	MyButton = MessageToDisplay.Show()
	If MyButton <= 0
		Return
	EndIf

	Int i = 0
	While i < ChemToUse.Length
		ButtonsAndChems selectedChem = ChemToUse[i]
		If selectedChem.ButtonPressed == MyButton
			If selectedChem.ItemToRemove != None && selectedChem.StageToSet > 0 && !owningQuest.IsStageDone(selectedChem.StageToSet) && playerRef.GetItemCount(selectedChem.ItemToRemove) > 0
				playerRef.RemoveItem(selectedChem.ItemToRemove, 1, True)
				If ConsoleReference != None && selectedChem.AnimationToPlay != ""
					ConsoleReference.PlayAnimation(selectedChem.AnimationToPlay)
				EndIf
				owningQuest.SetStage(selectedChem.StageToSet)
			EndIf
			Return
		EndIf
		i += 1
	EndWhile
EndEvent
