Event Scene.OnEnd(Scene akSender)
	If akSender == TW010_Intro && TW010_Intro.IsActionComplete(1) && !IsStageDone(150)
		SetStage(150)
	ElseIf akSender == TW010_TurnInMeat && TW010_TurnInMeat.IsActionComplete(1) && TW010_TurnInMeat.IsActionComplete(2) && !IsStageDone(500)
		SetStage(500)
	ElseIf akSender == TW010_TurnInVeges && TW010_TurnInVeges.IsActionComplete(1) && TW010_TurnInVeges.IsActionComplete(2) && !IsStageDone(1000)
		SetStage(1000)
	EndIf
EndEvent

Function Fragment_Stage_0040_Item_00()
	SetObjectiveDisplayed(120, True)
EndFunction

Function Fragment_Stage_0050_Item_00()
	SetObjectiveCompleted(120, True)
EndFunction

Function Fragment_Stage_0055_Item_00()
	SetObjectiveCompleted(130, True)
EndFunction

Function Fragment_Stage_0060_Item_00()
	SetObjectiveCompleted(510, True)
EndFunction

Function Fragment_Stage_0065_Item_00()
	SetObjectiveCompleted(520, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
	If TW010_Intro != None
		RegisterForRemoteEvent(TW010_Intro, "OnEnd")
	EndIf
	If TW010_TurnInMeat != None
		RegisterForRemoteEvent(TW010_TurnInMeat, "OnEnd")
	EndIf
	If TW010_TurnInVeges != None
		RegisterForRemoteEvent(TW010_TurnInVeges, "OnEnd")
	EndIf
	If Alias_MeatEnableMarker != None && Alias_MeatEnableMarker.GetReference() != None
		Alias_MeatEnableMarker.GetReference().Disable()
	EndIf
	If Alias_VegeEnableMarker != None && Alias_VegeEnableMarker.GetReference() != None
		Alias_VegeEnableMarker.GetReference().Disable()
	EndIf
	SetObjectiveDisplayed(50, True)
	If TW010_Intro != None && !TW010_Intro.IsPlaying()
		TW010_Intro.Start()
	EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
	SetObjectiveCompleted(50, True)
	SetObjectiveDisplayed(100, True)
	SetObjectiveDisplayed(110, True)
	If Alias_MeatEnableMarker != None && Alias_MeatEnableMarker.GetReference() != None
		Alias_MeatEnableMarker.GetReference().Enable()
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef != None && Perception != None && playerRef.GetValue(Perception) >= PerceptionThreshhold
		SetStage(40)
	EndIf

	DefaultAliasInventoryManagementA yaoGuaiInventory = Alias_Player as DefaultAliasInventoryManagementA
	If yaoGuaiInventory != None
		yaoGuaiInventory.EvaluateInventoryState()
	EndIf
	DefaultAliasInventoryManagementB deathclawInventory = Alias_Player as DefaultAliasInventoryManagementB
	If deathclawInventory != None
		deathclawInventory.EvaluateInventoryState()
	EndIf
	DefaultAliasInventoryManagementF radstagInventory = Alias_Player as DefaultAliasInventoryManagementF
	If radstagInventory != None
		radstagInventory.EvaluateInventoryState()
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(110, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(100, True)
	SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(300, True)
	If !IsObjectiveCompleted(120)
		SetObjectiveDisplayed(120, False)
	EndIf
	If !IsObjectiveCompleted(130)
		SetObjectiveDisplayed(130, False)
	EndIf
	SetObjectiveDisplayed(400, True)
	If Alias_MeatEnableMarker != None && Alias_MeatEnableMarker.GetReference() != None
		Alias_MeatEnableMarker.GetReference().Disable()
	EndIf
	If TW010_TurnInMeat != None && !TW010_TurnInMeat.IsPlaying()
		TW010_TurnInMeat.Start()
	EndIf
EndFunction

Function Fragment_Stage_0410_Item_00()
	SetObjectiveCompleted(120, True)
EndFunction

Function Fragment_Stage_0420_Item_00()
	SetObjectiveCompleted(130, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(400, True)
	SetObjectiveDisplayed(500, True)
	SetObjectiveDisplayed(510, True)
	SetObjectiveDisplayed(520, True)
	If Alias_VegeEnableMarker != None && Alias_VegeEnableMarker.GetReference() != None
		Alias_VegeEnableMarker.GetReference().Enable()
	EndIf
	If TW010_SideDishes != None && !TW010_SideDishes.IsPlaying()
		TW010_SideDishes.Start()
	EndIf

	DefaultAliasInventoryManagementD cornInventory = Alias_Player as DefaultAliasInventoryManagementD
	If cornInventory != None
		cornInventory.EvaluateInventoryState()
	EndIf
	DefaultAliasInventoryManagementE carrotInventory = Alias_Player as DefaultAliasInventoryManagementE
	If carrotInventory != None
		carrotInventory.EvaluateInventoryState()
	EndIf
	DefaultAliasInventoryManagementC tatoInventory = Alias_Player as DefaultAliasInventoryManagementC
	If tatoInventory != None
		tatoInventory.EvaluateInventoryState()
	EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(500, True)
	SetObjectiveDisplayed(600, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(600, True)
	If !IsObjectiveCompleted(510)
		SetObjectiveDisplayed(510, False)
	EndIf
	If !IsObjectiveCompleted(520)
		SetObjectiveDisplayed(520, False)
	EndIf
	SetObjectiveDisplayed(400, True, True)
	If Alias_VegeEnableMarker != None && Alias_VegeEnableMarker.GetReference() != None
		Alias_VegeEnableMarker.GetReference().Disable()
	EndIf
	If TW010_TurnInVeges != None && !TW010_TurnInVeges.IsPlaying()
		TW010_TurnInVeges.Start()
	EndIf
EndFunction

Function Fragment_Stage_0710_Item_00()
	SetObjectiveCompleted(510, True)
EndFunction

Function Fragment_Stage_0720_Item_00()
	SetObjectiveCompleted(520, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(400, True)
	If Alias_MeatEnableMarker != None && Alias_MeatEnableMarker.GetReference() != None
		Alias_MeatEnableMarker.GetReference().Disable()
	EndIf
	If Alias_VegeEnableMarker != None && Alias_VegeEnableMarker.GetReference() != None
		Alias_VegeEnableMarker.GetReference().Disable()
	EndIf

	Actor playerRef = None
	If Alias_Player != None
		playerRef = Alias_Player.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && TW010Status != None && GameDaysPassed != None
		playerRef.SetValue(TW010Status, GameDaysPassed.GetValue())
	EndIf
	Stop()
EndFunction
