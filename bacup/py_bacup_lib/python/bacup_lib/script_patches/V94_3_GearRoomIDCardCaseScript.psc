Event OnInit()
	InitializeCardCase()
EndEvent

Event OnLoad()
	InitializeCardCase()
EndEvent

Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	If akActionRef != playerRef || IsActivationBlocked()
		Return
	EndIf

	BlockActivation(True, True)
	GoToState("processingactivation")
	InitializeCardCase()

	If myV94_3QI == None || myV94_3QI.V94_3_SecurityIDCard == None
		FinishActivation()
		Return
	EndIf

	If playerRef.GetItemCount(myV94_3QI.V94_3_SecurityIDCard) > 0
		CompleteSecurityIdPickup()
		If V94_3_GearRoomIDCardCaseMessageHasCard != None
			V94_3_GearRoomIDCardCaseMessageHasCard.Show()
		EndIf
		FinishActivation()
		Return
	EndIf

	playerRef.AddItem(myV94_3QI.V94_3_SecurityIDCard, 1, True)
	If ITMKeycardPickup != None
		ITMKeycardPickup.Play(Self)
	EndIf
	HideNextFauxIdCard()
	CompleteSecurityIdPickup()
	FinishActivation()
EndEvent

Function InitializeCardCase()
	If myV94_3QI == None
		myV94_3QI = Game.GetFormFromFile(0x0046F0DD, "SeventySix.esm") as V94_3_VaultMissionQuestScript_Access
	EndIf
	If myFauxIDCards == None || myFauxIDCards.Length == 0
		myFauxIDCards = GetLinkedRefChain()
	EndIf
	If GetState() == ""
		GoToState("waitingforactivation")
	EndIf
EndFunction

Function HideNextFauxIdCard()
	If myFauxIDCards == None || myFauxIDCardIndex < 0 || myFauxIDCardIndex >= myFauxIDCards.Length
		Return
	EndIf
	If myFauxIDCards[myFauxIDCardIndex] != None
		myFauxIDCards[myFauxIDCardIndex].Disable()
	EndIf
	myFauxIDCardIndex += 1
EndFunction

Function CompleteSecurityIdPickup()
	Int completionStage = myV94_3QI.Entry_GearRoomIDCardPickupCompleteStage
	If completionStage > 0 && !myV94_3QI.IsStageDone(completionStage)
		myV94_3QI.SetStage(completionStage)
	EndIf
	myV94_3QI.SetObjectiveCompleted(20)
	myV94_3QI.SetObjectiveDisplayed(21)
EndFunction

Function FinishActivation()
	GoToState("waitingforactivation")
	BlockActivation(False)
EndFunction

State processingactivation
	Event OnActivate(ObjectReference akActionRef)
	EndEvent
EndState
