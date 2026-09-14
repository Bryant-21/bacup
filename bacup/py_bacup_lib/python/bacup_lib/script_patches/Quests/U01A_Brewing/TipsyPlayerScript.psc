Event OnAliasInit()
	PlayerRef = GetActorReference()
	OwningQuest = GetOwningQuest()
	QS = OwningQuest as Quests:U01A_Brewing:DailyScript
	RefreshSelectedStat()
	AddInventoryEventFilter(None)
EndEvent

Function RefreshSelectedStat()
	If PlayerRef == None
		PlayerRef = GetActorReference()
	EndIf
	If OwningQuest == None
		OwningQuest = GetOwningQuest()
	EndIf
	If QS == None
		QS = OwningQuest as Quests:U01A_Brewing:DailyScript
	EndIf
	If QS != None && QS.ChosenStatIndex >= 0 && QS.ChosenStatIndex < TestProperties.Length
		SelectedStat = TestProperties[QS.ChosenStatIndex]
	EndIf
EndFunction

Bool Function HasRequiredAlcoholEffect()
	Return PlayerRef != None && QS != None && QS.ChosenAlcohol.DurationEffect != None && PlayerRef.HasMagicEffect(QS.ChosenAlcohol.DurationEffect)
EndFunction

Bool Function HasDrunkRequiredAlcohol()
	If OwningQuest == None
		Return False
	EndIf
	Return OwningQuest.IsObjectiveCompleted(Obj_DrinkAlcohol) || OwningQuest.IsObjectiveCompleted(Obj_DrinkAlcoholNoVintage)
EndFunction

Function RevealSelectedTest()
	If OwningQuest != None
		OwningQuest.SetObjectiveDisplayed(SelectedStat.Objective, True)
	EndIf
EndFunction

Function CompleteSelectedTest()
	If OwningQuest == None || OwningQuest.GetStageDone(TestCompletedStage) || !HasDrunkRequiredAlcohol() || !HasRequiredAlcoholEffect()
		Return
	EndIf
	OwningQuest.SetStage(TestCompletedStage)
EndFunction

Event Actor.OnItemEquipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
	If akSender != PlayerRef || QS == None || akBaseObject == None
		Return
	EndIf

	If akBaseObject == QS.ChosenAlcohol.FreshDrink || (QS.ChosenAlcohol.VintageDrink != None && akBaseObject == QS.ChosenAlcohol.VintageDrink)
		If QS.ChosenAlcohol.VintageDrink != None
			OwningQuest.SetObjectiveCompleted(Obj_DrinkAlcohol, True)
		Else
			OwningQuest.SetObjectiveCompleted(Obj_DrinkAlcoholNoVintage, True)
		EndIf
		RevealSelectedTest()
		Return
	EndIf

	If SelectedStat.PrereqStage == 400 && SelectedStat.Keyword1 != None && akBaseObject.HasKeyword(SelectedStat.Keyword1)
		CompleteSelectedTest()
	EndIf
EndEvent

Event Actor.OnKill(Actor akSender, Actor akVictim)
	If akSender != PlayerRef || !HasRequiredAlcoholEffect()
		Return
	EndIf

	Weapon equippedWeapon = PlayerRef.GetEquippedWeapon()
	If SelectedStat.PrereqStage == 200
		If equippedWeapon == None || (SelectedStat.Keyword1 != None && equippedWeapon.HasKeyword(SelectedStat.Keyword1)) || (SelectedStat.Keyword2 != None && equippedWeapon.HasKeyword(SelectedStat.Keyword2))
			CompleteSelectedTest()
		EndIf
	ElseIf SelectedStat.PrereqStage == 300
		If equippedWeapon != None && SelectedStat.Keyword1 != None && equippedWeapon.HasKeyword(SelectedStat.Keyword1)
			CompleteSelectedTest()
		EndIf
	ElseIf SelectedStat.PrereqStage == 700 && PlayerRef.IsSneaking()
		CompleteSelectedTest()
	ElseIf SelectedStat.PrereqStage == 800
		; Fallout 4 exposes no Papyrus critical-hit event, so an effect-qualified kill is the reliable local substitute.
		CompleteSelectedTest()
	EndIf
EndEvent

Event Actor.OnPlayerUseWorkBench(Actor akSender, ObjectReference akWorkBench)
	If akSender == PlayerRef && SelectedStat.PrereqStage == 600
		CompleteSelectedTest()
	EndIf
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If SelectedStat.PrereqStage == 500 && akSourceContainer != None
		CompleteSelectedTest()
	EndIf
EndEvent
