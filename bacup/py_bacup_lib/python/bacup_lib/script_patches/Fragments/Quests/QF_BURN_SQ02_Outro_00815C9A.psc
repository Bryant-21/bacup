ObjectReference Function PlayerReference()
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	Return playerRef
EndFunction

Function SetPlayerValue(ActorValue valueToSet, Float value)
	Actor player = PlayerReference() as Actor
	If player != None && valueToSet != None
		player.SetValue(valueToSet, value)
	EndIf
EndFunction

Function StartBoundScene(Scene sceneToStart)
	If sceneToStart != None && !sceneToStart.IsPlaying()
		sceneToStart.Start()
	EndIf
EndFunction

Function StopBoundScene(Scene sceneToStop)
	If sceneToStop != None && sceneToStop.IsPlaying()
		sceneToStop.Stop()
	EndIf
EndFunction

Function AddItemOnce(ObjectReference containerRef, Form itemToAdd)
	If containerRef != None && itemToAdd != None && containerRef.GetItemCount(itemToAdd) == 0
		containerRef.AddItem(itemToAdd, 1, True)
	EndIf
EndFunction

Function RemovePlayerItem(Form itemToRemove)
	ObjectReference playerRef = PlayerReference()
	If playerRef != None && itemToRemove != None
		Int itemCount = playerRef.GetItemCount(itemToRemove)
		If itemCount > 0
			playerRef.RemoveItem(itemToRemove, itemCount, True)
		EndIf
	EndIf
EndFunction

Function EnableAlias(ReferenceAlias aliasToEnable)
	If aliasToEnable != None
		aliasToEnable.TryToEnable()
	EndIf
EndFunction

Function DisableAlias(ReferenceAlias aliasToDisable)
	If aliasToDisable != None
		aliasToDisable.TryToDisable()
	EndIf
EndFunction

Function EvaluateAliasActor(ReferenceAlias actorAlias)
	If actorAlias != None
		Actor actorRef = actorAlias.GetActorReference()
		If actorRef != None
			actorRef.EvaluatePackage()
		EndIf
	EndIf
EndFunction

Function MakeAliasHostile(ReferenceAlias actorAlias)
	If actorAlias == None
		Return
	EndIf
	Actor actorRef = actorAlias.GetActorReference()
	Actor playerRef = PlayerReference() as Actor
	If actorRef != None
		If Faction_PlayerFriendlyFaction != None
			actorRef.RemoveFromFaction(Faction_PlayerFriendlyFaction)
		EndIf
		If Faction_PlayerEnemyFaction != None && !actorRef.IsInFaction(Faction_PlayerEnemyFaction)
			actorRef.AddToFaction(Faction_PlayerEnemyFaction)
		EndIf
		If playerRef != None && !actorRef.IsDead()
			actorRef.StartCombat(playerRef)
		EndIf
	EndIf
EndFunction

Function AdvanceIfPending(Int nextStage)
	If !IsStageDone(nextStage)
		SetStage(nextStage)
	EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
	Return
EndFunction

Function Fragment_Stage_0020_Item_00()
	Return
EndFunction

Function Fragment_Stage_0030_Item_00()
	Return
EndFunction

Function Fragment_Stage_0040_Item_00()
	Return
EndFunction

Function Fragment_Stage_0050_Item_00()
	Return
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0110_Item_00()
	Return
EndFunction

Function Fragment_Stage_0150_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0175_Item_00()
	SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(15)
	SetObjectiveDisplayed(20)
	SetPlayerValue(AV_Runt, 0.0)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(40)
	ObjectReference underbellyDoor = Door_Underbelly.GetReference()
	If underbellyDoor != None
		underbellyDoor.Lock(False)
		underbellyDoor.BlockActivation(False)
	EndIf
	EvaluateAliasActor(Actor_Moose_HWT)
EndFunction

Function Fragment_Stage_0405_Item_00()
	Return
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveDisplayed(60)
	EnableAlias(Enemy_PatrolCaptain)
	EvaluateAliasActor(Enemy_PatrolCaptain)
EndFunction

Function Fragment_Stage_0680_Item_00()
	AdvanceIfPending(700)
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(60)
	SetObjectiveDisplayed(70)
	ObjectReference supplyContainer = Container_Raiders.GetReference()
	AddItemOnce(supplyContainer, Item_SuppliesBackpack)
	DisableAlias(Enemy_PatrolCaptain)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(70)
	SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0810_Item_00()
	RemovePlayerItem(Item_SuppliesBackpack)
EndFunction

Function Fragment_Stage_0815_Item_00()
	Return
EndFunction

Function Fragment_Stage_0850_Item_00()
	AdvanceIfPending(900)
EndFunction

Function Fragment_Stage_0851_Item_00()
	AdvanceIfPending(900)
EndFunction

Function Fragment_Stage_0900_Item_00()
	SetObjectiveCompleted(80)
	SetObjectiveDisplayed(90)
	SetPlayerValue(AV_EugeneHWT, 1.0)
	EnableAlias(Actor_Eugene_HWT)
	EvaluateAliasActor(Actor_Eugene_HWT)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(90)
	SetObjectiveDisplayed(100)
	EnableAlias(Actor_Leonard_Railyard)
	EvaluateAliasActor(Actor_Leonard_Railyard)
EndFunction

Function Fragment_Stage_1050_Item_00()
	Return
EndFunction

Function Fragment_Stage_1100_Item_00()
	SetObjectiveCompleted(100)
	SetObjectiveDisplayed(110)
	EnableAlias(Corpse_Owen)
	EnableAlias(Corpse_NPC)
	EnableAlias(Corpse_Raider)
	AddItemOnce(Corpse_Owen.GetReference(), Item_Ring)
EndFunction

Function Fragment_Stage_1110_Item_00()
	Return
EndFunction

Function Fragment_Stage_1120_Item_00()
	Return
EndFunction

Function Fragment_Stage_1130_Item_00()
	Return
EndFunction

Function Fragment_Stage_1200_Item_00()
	SetObjectiveCompleted(110)
	SetObjectiveDisplayed(120)
	SetObjectiveDisplayed(125)
EndFunction

Function Fragment_Stage_1300_Item_00()
	SetObjectiveCompleted(120)
	SetObjectiveDisplayed(130)
EndFunction

Function Fragment_Stage_1303_Item_00()
	SetObjectiveCompleted(125)
EndFunction

Function Fragment_Stage_1305_Item_00()
	SetObjectiveCompleted(130)
	If IsObjectiveDisplayed(125) && !IsObjectiveCompleted(125)
		SetObjectiveFailed(125)
	EndIf
	RemovePlayerItem(Item_Ring)
EndFunction

Function Fragment_Stage_1310_Item_00()
	SetPlayerValue(AV_LeonardChoice, 1.0)
EndFunction

Function Fragment_Stage_1320_Item_00()
	SetPlayerValue(AV_LeonardChoice, 2.0)
EndFunction

Function Fragment_Stage_1350_Item_00()
	AdvanceIfPending(1400)
EndFunction

Function Fragment_Stage_1351_Item_00()
	AdvanceIfPending(1400)
EndFunction

Function Fragment_Stage_1400_Item_00()
	SetObjectiveCompleted(130)
	SetObjectiveDisplayed(140)
	SetPlayerValue(AV_MagpieBasement, 1.0)
	EnableAlias(Actor_Magpie_Basement)
	EvaluateAliasActor(Actor_Magpie_Basement)
EndFunction

Function Fragment_Stage_1500_Item_00()
	SetObjectiveCompleted(140)
	SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_1505_Item_00()
	Return
EndFunction

Function Fragment_Stage_1507_Item_00()
	Return
EndFunction

Function Fragment_Stage_1510_Item_00()
	AddItemOnce(PlayerReference(), Book_Caps)
EndFunction

Function Fragment_Stage_1520_Item_00()
	Return
EndFunction

Function Fragment_Stage_1525_Item_00()
	Return
EndFunction

Function Fragment_Stage_1530_Item_00()
	Return
EndFunction

Function Fragment_Stage_1600_Item_00()
	SetObjectiveCompleted(150)
	SetObjectiveDisplayed(160)
EndFunction

Function Fragment_Stage_1700_Item_00()
	SetObjectiveCompleted(160)
	SetObjectiveDisplayed(170)
EndFunction

Function Fragment_Stage_1800_Item_00()
	SetObjectiveCompleted(170)
	SetObjectiveDisplayed(180)
	EnableAlias(Actor_LoanShark)
	EvaluateAliasActor(Actor_LoanShark)
	StartBoundScene(Scene_LoanShark_Introduction)
EndFunction

Function Fragment_Stage_1810_Item_00()
	Return
EndFunction

Function Fragment_Stage_1815_Item_00()
	RemovePlayerItem(Item_CapsStash)
EndFunction

Function Fragment_Stage_1820_Item_00()
	SetObjectiveCompleted(180)
	SetObjectiveDisplayed(185)
	StopBoundScene(Scene_LoanShark_Introduction)
	MakeAliasHostile(Enemy_Thug_01)
	MakeAliasHostile(Enemy_Thug_02)
	MakeAliasHostile(Enemy_Thug_03)
	MakeAliasHostile(Enemy_Turret)
EndFunction

Function CompleteLoanSharkThugsIfDead()
	If IsStageDone(1823) && IsStageDone(1826) && IsStageDone(1829)
		AdvanceIfPending(1830)
	EndIf
EndFunction

Function Fragment_Stage_1823_Item_00()
	CompleteLoanSharkThugsIfDead()
EndFunction

Function Fragment_Stage_1826_Item_00()
	CompleteLoanSharkThugsIfDead()
EndFunction

Function Fragment_Stage_1829_Item_00()
	CompleteLoanSharkThugsIfDead()
EndFunction

Function Fragment_Stage_1830_Item_00()
	SetObjectiveCompleted(185)
	SetObjectiveDisplayed(186)
	AddItemOnce(Container_LoanShark.GetReference(), Item_LoanKey)
EndFunction

Function Fragment_Stage_1833_Item_00()
	SetObjectiveCompleted(186)
	SetObjectiveDisplayed(187)
EndFunction

Function Fragment_Stage_1835_Item_00()
	SetObjectiveCompleted(187)
	SetObjectiveDisplayed(188)
	StartBoundScene(Scene_LoanShark_Attack)
EndFunction

Function Fragment_Stage_1850_Item_00()
	SetObjectiveCompleted(188)
	SetObjectiveDisplayed(189)
	StopBoundScene(Scene_LoanShark_Attack)
	MakeAliasHostile(Actor_LoanShark)
EndFunction

Function Fragment_Stage_1890_Item_00()
	SetPlayerValue(AV_LoanShark, 1.0)
	AdvanceIfPending(1900)
EndFunction

Function Fragment_Stage_1895_Item_00()
	SetPlayerValue(AV_LoanShark, 2.0)
	AdvanceIfPending(1900)
EndFunction

Function Fragment_Stage_1900_Item_00()
	If Actor_LoanShark.GetActorReference() != None && Actor_LoanShark.GetActorReference().IsDead()
		SetPlayerValue(AV_LoanShark, 3.0)
	EndIf
	SetObjectiveCompleted(180)
	SetObjectiveCompleted(185)
	SetObjectiveCompleted(186)
	SetObjectiveCompleted(187)
	SetObjectiveCompleted(188)
	SetObjectiveCompleted(189)
	SetObjectiveDisplayed(190)
	SetPlayerValue(AV_EugeneBasement, 1.0)
	EnableAlias(Actor_Eugene_Basement)
	EvaluateAliasActor(Actor_Eugene_Basement)
EndFunction

Function Fragment_Stage_1910_Item_00()
	SetPlayerValue(AV_MagpieChoice, 1.0)
EndFunction

Function Fragment_Stage_1920_Item_00()
	SetPlayerValue(AV_MagpieChoice, 2.0)
EndFunction

Function Fragment_Stage_1950_Item_00()
	AdvanceIfPending(2000)
EndFunction

Function Fragment_Stage_1951_Item_00()
	AdvanceIfPending(2000)
EndFunction

Function Fragment_Stage_2000_Item_00()
	SetObjectiveCompleted(190)
	SetObjectiveDisplayed(200)
	SetPlayerValue(AV_EugeneHWT, 0.0)
	SetPlayerValue(AV_EugeneBasement, 1.0)
	DisableAlias(Actor_Eugene_HWT)
	EnableAlias(Actor_Eugene_Basement)
	EvaluateAliasActor(Actor_Eugene_Basement)
EndFunction

Function Fragment_Stage_2010_Item_00()
	AddItemOnce(PlayerReference(), Key_Pipe)
EndFunction

Function Fragment_Stage_9000_Item_00()
	CompleteAllObjectives()
	ObjectReference playerRef = PlayerReference()
	If BURN_SQ02_OutroP2_QuestStartKeyword != None && playerRef != None
		BURN_SQ02_OutroP2_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	EndIf
	AdvanceIfPending(9999)
EndFunction

Function Fragment_Stage_9999_Item_00()
	StopBoundScene(Scene_LoanShark_Introduction)
	StopBoundScene(Scene_LoanShark_Attack)
	Stop()
EndFunction
