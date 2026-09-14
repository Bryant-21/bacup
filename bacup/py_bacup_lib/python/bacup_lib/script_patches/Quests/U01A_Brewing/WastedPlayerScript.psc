Event OnAliasInit()
	OwningQuest = GetOwningQuest()
	QS = OwningQuest as Quests:U01A_Brewing:DailyScript
	AddInventoryEventFilter(None)
	RefreshObjectives()
EndEvent

Function RefreshObjectives()
	If OwningQuest == None
		OwningQuest = GetOwningQuest()
	EndIf
	If QS == None
		QS = OwningQuest as Quests:U01A_Brewing:DailyScript
	EndIf
	Actor playerRef = GetActorReference()
	If playerRef == None || OwningQuest == None || QS == None
		Return
	EndIf

	Bool hasFermentingDrink = QS.ChosenAlcohol.FermDrink != None && playerRef.GetItemCount(QS.ChosenAlcohol.FermDrink) > 0
	Bool hasReadyDrink = QS.PlayerHasReadyAlcohol()
	If hasFermentingDrink
		OwningQuest.SetObjectiveCompleted(Obj_Craft, True)
		OwningQuest.SetObjectiveDisplayed(Obj_Ferment, True)
		OwningQuest.SetObjectiveDisplayed(Obj_FermentTip, True)
	EndIf

	If hasReadyDrink
		OwningQuest.SetObjectiveCompleted(Obj_Craft, True)
		OwningQuest.SetObjectiveCompleted(Obj_Ferment, True)
		OwningQuest.SetObjectiveCompleted(Obj_FermentTip, True)
		OwningQuest.SetObjectiveCompleted(Obj_RetrieveFromStash, True)
		OwningQuest.SetObjectiveDisplayed(Obj_TurnIn, True)
		QS.HasAlcohol = True
	Else
		OwningQuest.SetObjectiveDisplayed(Obj_TurnIn, False)
		QS.HasAlcohol = False
	EndIf
EndFunction

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If QS == None || akBaseItem == None
		Return
	EndIf

	If akBaseItem == QS.ChosenAlcohol.FermDrink
		OwningQuest.SetObjectiveCompleted(Obj_Craft, True)
		OwningQuest.SetObjectiveDisplayed(Obj_Ferment, True)
		OwningQuest.SetObjectiveDisplayed(Obj_FermentTip, True)
	ElseIf akBaseItem == QS.ChosenAlcohol.FreshDrink || (QS.ChosenAlcohol.VintageDrink != None && akBaseItem == QS.ChosenAlcohol.VintageDrink)
		OwningQuest.SetObjectiveCompleted(Obj_Craft, True)
		OwningQuest.SetObjectiveCompleted(Obj_Ferment, True)
		OwningQuest.SetObjectiveCompleted(Obj_FermentTip, True)
		OwningQuest.SetObjectiveCompleted(Obj_RetrieveFromStash, True)
	EndIf
	RefreshObjectives()
EndEvent

Event OnItemRemoved(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
	If QS == None || akBaseItem == None
		Return
	EndIf

	If akBaseItem == QS.ChosenAlcohol.FreshDrink || (QS.ChosenAlcohol.VintageDrink != None && akBaseItem == QS.ChosenAlcohol.VintageDrink)
		If !QS.PlayerHasReadyAlcohol()
			OwningQuest.SetObjectiveDisplayed(Obj_RetrieveFromStash, True)
		EndIf
	EndIf
	RefreshObjectives()
EndEvent
