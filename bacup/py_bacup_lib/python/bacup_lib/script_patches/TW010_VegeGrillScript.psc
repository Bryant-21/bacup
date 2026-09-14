Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	Quest owningQuest = GetOwningQuest()
	If playerRef == None || akActionRef != playerRef || owningQuest == None
		Return
	EndIf
	If owningQuest.IsStageDone(StageToSetVegetables)
		Return
	EndIf
	If !owningQuest.IsStageDone(PrereqStageVegetables) || Tato == None || playerRef.GetItemCount(Tato) < TatoCount
		If TW010GrillNotReadyMsg != None
			TW010GrillNotReadyMsg.Show()
		EndIf
		Return
	EndIf

	Bool turnInCorn = Corn != None && playerRef.GetItemCount(Corn) >= CornCount
	Bool turnInCarrots = Carrot != None && playerRef.GetItemCount(Carrot) >= CarrotCount

	playerRef.RemoveItem(Tato, TatoCount, True)
	If turnInCorn
		playerRef.RemoveItem(Corn, CornCount, True)
	EndIf
	If turnInCarrots
		playerRef.RemoveItem(Carrot, CarrotCount, True)
	EndIf

	owningQuest.SetStage(StageToSetVegetables)
	If turnInCorn
		owningQuest.SetStage(CornStage)
	EndIf
	If turnInCarrots
		owningQuest.SetStage(CarrotsStage)
	EndIf
EndEvent
