Function Fragment_Stage_1000_Item_00()
	SetStage(1100)
EndFunction

Function Fragment_Stage_1100_Item_00()
	SetObjectiveDisplayed(5)
EndFunction

Function Fragment_Stage_1200_Item_00()
	SetObjectiveCompleted(5)
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_2000_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)

	Actor playerRef = Alias_Player.GetActorReference()
	Weapon meltdownCarbine = Game.GetFormFromFile(0x006F5790, "SeventySix.esm") as Weapon
	If playerRef != None && meltdownCarbine != None
		If playerRef.GetItemCount(meltdownCarbine) < 1
			playerRef.AddItem(meltdownCarbine, 1, True)
		EndIf
		If !IsStageDone(3000)
			SetStage(3000)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_3000_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)

	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef != None && MILE_SQ_CraftedMeltdownAV != None
		playerRef.SetValue(MILE_SQ_CraftedMeltdownAV, 1.0)
	EndIf
EndFunction

Function Fragment_Stage_4000_Item_00()
	SetObjectiveCompleted(30)
	SetStage(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && MeltdownSchematics != None
		playerRef.RemoveItem(MeltdownSchematics, 1, True)
	EndIf
	Stop()
EndFunction
