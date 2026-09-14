Function Fragment_Stage_0010_Item_00()
	Actor playerRef = None
	If Alias_FF05_Balance_Player != None
		playerRef = Alias_FF05_Balance_Player.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf

	If playerRef != None && FF05_Balance_Started != None
		playerRef.SetValue(FF05_Balance_Started, 1.0)
	EndIf

	If playerRef != None && FF05_Balance_Completed != None && playerRef.GetValue(FF05_Balance_Completed) >= 1.0
		SetStage(50)
	Else
		SetObjectiveDisplayed(2, True)
	EndIf
EndFunction

Function Fragment_Stage_0015_Item_00()
	SetObjectiveCompleted(2, True)
	SetObjectiveDisplayed(3, True)
EndFunction

Function Fragment_Stage_0020_Item_00()
	SetObjectiveCompleted(3, True)
	SetObjectiveDisplayed(4, True)
EndFunction

Function Fragment_Stage_0025_Item_00()
	SetObjectiveCompleted(4, True)
	SetObjectiveDisplayed(6, True)
EndFunction

Function Fragment_Stage_0030_Item_00()
	SetObjectiveCompleted(6, True)
	SetObjectiveDisplayed(8, True)

	Actor playerRef = None
	If Alias_FF05_Balance_Player != None
		playerRef = Alias_FF05_Balance_Player.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && FF05_Balance_PasswordLearned != None
		playerRef.SetValue(FF05_Balance_PasswordLearned, 1.0)
	EndIf
EndFunction

Function Fragment_Stage_0040_Item_00()
	SetObjectiveCompleted(8, True)
	SetObjectiveDisplayed(9, True)
EndFunction

Function Fragment_Stage_0050_Item_00()
	If IsObjectiveDisplayed(9)
		SetObjectiveCompleted(9, True)
	EndIf
	SetObjectiveDisplayed(10, True)
	SetObjectiveDisplayed(20, True)
	SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveCompleted(10, True)
	SetObjectiveDisplayed(40, True)

	Actor playerRef = None
	If Alias_FF05_Balance_Player != None
		playerRef = Alias_FF05_Balance_Player.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	ObjectReference dataHolotape = None
	If Alias_WaterHolotape != None
		dataHolotape = Alias_WaterHolotape.GetReference()
	EndIf
	If playerRef != None && dataHolotape != None && playerRef.GetItemCount(dataHolotape) == 0
		playerRef.AddItem(dataHolotape, 1, False)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(20, True)
	SetObjectiveDisplayed(50, True)

	Actor playerRef = None
	If Alias_FF05_Balance_Player != None
		playerRef = Alias_FF05_Balance_Player.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	ObjectReference dataHolotape = None
	If Alias_SoilHolotape != None
		dataHolotape = Alias_SoilHolotape.GetReference()
	EndIf
	If playerRef != None && dataHolotape != None && playerRef.GetItemCount(dataHolotape) == 0
		playerRef.AddItem(dataHolotape, 1, False)
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(30, True)
	SetObjectiveDisplayed(60, True)

	Actor playerRef = None
	If Alias_FF05_Balance_Player != None
		playerRef = Alias_FF05_Balance_Player.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	ObjectReference dataHolotape = None
	If Alias_AirHolotape != None
		dataHolotape = Alias_AirHolotape.GetReference()
	EndIf
	If playerRef != None && dataHolotape != None && playerRef.GetItemCount(dataHolotape) == 0
		playerRef.AddItem(dataHolotape, 1, False)
	EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(40, True)

	Actor playerRef = None
	If Alias_FF05_Balance_Player != None
		playerRef = Alias_FF05_Balance_Player.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	ObjectReference dataHolotape = None
	If Alias_WaterHolotape != None
		dataHolotape = Alias_WaterHolotape.GetReference()
	EndIf
	If playerRef != None && dataHolotape != None && playerRef.GetItemCount(dataHolotape) > 0
		playerRef.RemoveItem(dataHolotape, 1, True)
	EndIf

	If IsStageDone(500) && IsStageDone(600) && !IsStageDone(650)
		SetStage(650)
	EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(50, True)

	Actor playerRef = None
	If Alias_FF05_Balance_Player != None
		playerRef = Alias_FF05_Balance_Player.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	ObjectReference dataHolotape = None
	If Alias_SoilHolotape != None
		dataHolotape = Alias_SoilHolotape.GetReference()
	EndIf
	If playerRef != None && dataHolotape != None && playerRef.GetItemCount(dataHolotape) > 0
		playerRef.RemoveItem(dataHolotape, 1, True)
	EndIf

	If IsStageDone(400) && IsStageDone(600) && !IsStageDone(650)
		SetStage(650)
	EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(60, True)

	Actor playerRef = None
	If Alias_FF05_Balance_Player != None
		playerRef = Alias_FF05_Balance_Player.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	ObjectReference dataHolotape = None
	If Alias_AirHolotape != None
		dataHolotape = Alias_AirHolotape.GetReference()
	EndIf
	If playerRef != None && dataHolotape != None && playerRef.GetItemCount(dataHolotape) > 0
		playerRef.RemoveItem(dataHolotape, 1, True)
	EndIf

	If IsStageDone(400) && IsStageDone(500) && !IsStageDone(650)
		SetStage(650)
	EndIf
EndFunction

Function Fragment_Stage_0650_Item_00()
	SetObjectiveDisplayed(70, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(70, True)

	Actor playerRef = None
	If Alias_FF05_Balance_Player != None
		playerRef = Alias_FF05_Balance_Player.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && FF05_Balance_Completed != None
		playerRef.SetValue(FF05_Balance_Completed, 1.0)
	EndIf

	Stop()
EndFunction

Function Fragment_Stage_2000_Item_00()
	Actor playerRef = None
	If Alias_FF05_Balance_Player != None
		playerRef = Alias_FF05_Balance_Player.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && FF05_Balance_Started != None
		playerRef.SetValue(FF05_Balance_Started, 0.0)
	EndIf
EndFunction
