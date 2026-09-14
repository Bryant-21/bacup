Event OnQuestInit()
	PlayerRef = Alias_Player.GetActorReference()
	If PlayerRef == None
		PlayerRef = Game.GetPlayer()
	EndIf

	MasterQuest = D01A_Brewing_MasterQuest
	MasterScript = MasterQuest as Quests:U01A_Brewing:MasterScript
	HasAlcohol = False

	If D01A_Wasted != None && GetFormID() == D01A_Wasted.GetFormID()
		InitializeWastedQuest()
	ElseIf D01A_Tipsy != None && GetFormID() == D01A_Tipsy.GetFormID()
		InitializeTipsyQuest()
	EndIf
EndEvent

Bool Function HasAvailableWastedRecipe(Actor akPlayer)
	Quests:U01A_Brewing:MasterScript master = D01A_Brewing_MasterQuest as Quests:U01A_Brewing:MasterScript
	If akPlayer == None || master == None || master.AllDrinks.Length == 0
		Return False
	EndIf

	Int recipeIndex = akPlayer.GetValue(D01A_ChosenAlcohol) as Int
	Return recipeIndex >= 0 && recipeIndex < master.AllDrinks.Length
EndFunction

Function InitializeWastedQuest()
	If PlayerRef == None || MasterScript == None || MasterScript.AllDrinks.Length == 0
		Return
	EndIf

	ChosenAlcoholIndex = PlayerRef.GetValue(D01A_ChosenAlcohol) as Int
	If ChosenAlcoholIndex < 0
		ChosenAlcoholIndex = 0
	EndIf
	If ChosenAlcoholIndex >= MasterScript.AllDrinks.Length
		Return
	EndIf

	ChosenAlcohol = MasterScript.AllDrinks[ChosenAlcoholIndex]
	If Alias_Loc_AlcoholName != None && ChosenAlcohol.NameLoc != None
		Alias_Loc_AlcoholName.ForceLocationTo(ChosenAlcohol.NameLoc)
	EndIf
	HasAlcohol = PlayerHasReadyAlcohol()
	Quests:U01A_Brewing:WastedPlayerScript playerScript = Alias_Player as Quests:U01A_Brewing:WastedPlayerScript
	If playerScript != None
		playerScript.RefreshObjectives()
	EndIf
EndFunction

Function InitializeTipsyQuest()
	If PlayerRef == None || MasterScript == None || MasterScript.AllDrinks.Length == 0 || SPECIAL.Length == 0
		Return
	EndIf

	Int drinkCount = MasterScript.AllDrinks.Length
	ChosenAlcoholIndex = Utility.RandomInt(0, drinkCount - 1)
	Int attempts = 0
	While MasterScript.AllDrinks[ChosenAlcoholIndex].ExcludeFromTipsy && attempts < drinkCount
		ChosenAlcoholIndex = (ChosenAlcoholIndex + 1) % drinkCount
		attempts += 1
	EndWhile
	ChosenAlcohol = MasterScript.AllDrinks[ChosenAlcoholIndex]

	ChosenStatIndex = Utility.RandomInt(0, SPECIAL.Length - 1)
	ChosenStat = SPECIAL[ChosenStatIndex]
	PlayerRef.SetValue(D01A_ChosenStat, ChosenStatIndex as Float)

	If Alias_Loc_AlcoholName != None && ChosenAlcohol.NameLoc != None
		Alias_Loc_AlcoholName.ForceLocationTo(ChosenAlcohol.NameLoc)
	EndIf
	If Alias_Loc_StatName != None && ChosenStat.NameLoc != None
		Alias_Loc_StatName.ForceLocationTo(ChosenStat.NameLoc)
	EndIf
	Quests:U01A_Brewing:TipsyPlayerScript playerScript = Alias_Player as Quests:U01A_Brewing:TipsyPlayerScript
	If playerScript != None
		playerScript.RefreshSelectedStat()
	EndIf
	SetStage(ChosenStat.Stage)
EndFunction

Function BeginTipsyTest()
	SetObjectiveDisplayed(10, True)
	If ChosenAlcohol.VintageDrink != None
		SetObjectiveDisplayed(15, True)
	Else
		SetObjectiveDisplayed(14, True)
	EndIf
EndFunction

Function CompleteTipsyTest()
	Int actionObjective = ChosenStat.Stage / 10
	SetObjectiveCompleted(actionObjective, True)
	SetObjectiveCompleted(10, True)
	SetObjectiveDisplayed(100, True)
EndFunction

Bool Function PlayerHasReadyAlcohol()
	If PlayerRef == None
		Return False
	EndIf

	If ChosenAlcohol.FreshDrink != None && PlayerRef.GetItemCount(ChosenAlcohol.FreshDrink) > 0
		Return True
	EndIf
	Return ChosenAlcohol.VintageDrink != None && PlayerRef.GetItemCount(ChosenAlcohol.VintageDrink) > 0
EndFunction

Function CompleteDailyQuest(Bool abAdvanceAlcoholRecipe = False)
	If PlayerRef == None
		PlayerRef = Game.GetPlayer()
	EndIf
	If PlayerRef == None
		Return
	EndIf

	PlayerRef.SetValue(D01A_BrewingDailyTimestamp, Utility.GetCurrentGameTime())
	If abAdvanceAlcoholRecipe
		Int nextRecipeIndex = (PlayerRef.GetValue(D01A_ChosenAlcohol) as Int) + 1
		If MasterScript != None && nextRecipeIndex > MasterScript.AllDrinks.Length
			nextRecipeIndex = MasterScript.AllDrinks.Length
		EndIf
		PlayerRef.SetValue(D01A_ChosenAlcohol, nextRecipeIndex as Float)
	EndIf
	HasAlcohol = False
EndFunction
