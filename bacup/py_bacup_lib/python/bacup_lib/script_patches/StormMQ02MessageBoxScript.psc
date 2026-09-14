Event OnActivate(ObjectReference akActionRef)
	ObjectReference playerRef = PlayerAlias.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If akActionRef != playerRef
		Return
	EndIf

	Message messageToShow = FallbackMessage
	Int highestRequirement = -1
	Int index = 0
	While Messages != None && index < Messages.Length
		If Messages[index].ShownMessage != None && Messages[index].TargetAV != None && playerRef.GetValue(Messages[index].TargetAV) >= Messages[index].RequiredAmount && Messages[index].RequiredAmount > highestRequirement
			messageToShow = Messages[index].ShownMessage
			highestRequirement = Messages[index].RequiredAmount
		EndIf
		index += 1
	EndWhile
	If messageToShow == None
		Return
	EndIf

	Int selectedButton = messageToShow.Show()
	If ButtonStagesToSet == None || selectedButton < 0 || selectedButton >= ButtonStagesToSet.Length
		Return
	EndIf
	Quest owningQuest = GetOwningQuest()
	Int stageToSet = ButtonStagesToSet[selectedButton]
	If owningQuest != None && stageToSet > 0 && !owningQuest.IsStageDone(stageToSet)
		owningQuest.SetStage(stageToSet)
	EndIf
EndEvent
