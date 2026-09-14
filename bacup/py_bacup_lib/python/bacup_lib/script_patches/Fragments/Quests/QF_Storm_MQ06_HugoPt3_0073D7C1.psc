ObjectReference Function PlayerReference()
	Return Alias_Player.GetReference()
EndFunction

Function MakeCultistsHostile()
	Int index = 0
	While index < Alias_RefCol_AllCultists_Kill.GetCount()
		Actor cultist = Alias_RefCol_AllCultists_Kill.GetAt(index) as Actor
		If cultist
			cultist.AddToFaction(PlayerEnemyFaction)
			cultist.SetValue(Agression, 2.0)
			cultist.EvaluatePackage()
		EndIf
		index += 1
	EndWhile
EndFunction

Function EndDisguise()
	Actor player = Alias_Player.GetActorReference()
	If player
		player.RemoveFromFaction(StormCultistFaction)
	EndIf
EndFunction

Function Fragment_Stage_0090_Item_00()
	ObjectReference player = PlayerReference()
	player.SetValue(Storm_MQ_AudreyAwayValue, 1.0)
	player.SetValue(Storm_MQ_HugoAwayValue, 1.0)
EndFunction

Function Fragment_Stage_0010_Item_00()
	If IsStageDone(1100) && !IsStageDone(1110)
		EndDisguise()
		MakeCultistsHostile()
		SetObjectiveDisplayed(70)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	Actor audrey = Alias_Actor_Audrey.GetActorReference()
	ObjectReference destination = Alias_Ref_AudreyTeleport_Stage50.GetReference()
	If audrey
		If destination
			audrey.MoveTo(destination)
		EndIf
		audrey.EvaluatePackage()
	EndIf
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0110_Item_00()
	Actor hugo = Alias_Actor_Hugo_WeatherLab.GetActorReference()
	If hugo
		hugo.Enable()
		ObjectReference audrey = Alias_Actor_Audrey.GetReference()
		If audrey
			hugo.MoveTo(audrey)
		EndIf
		hugo.EvaluatePackage()
	EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
	ObjectReference player = PlayerReference()
	If player.GetItemCount(Misc_TrapBook) == 0
		player.AddItem(Misc_TrapBook, 1, True)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0310_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(35)
EndFunction

Function Fragment_Stage_0320_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveCompleted(35)
	SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveCompleted(35)
	SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0450_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(42)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(42)
	SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_0550_Item_00()
	SetObjectiveCompleted(45)
	SetObjectiveDisplayed(47)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(45)
	SetObjectiveCompleted(47)
	SetObjectiveDisplayed(46)
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(46)
	SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0900_Item_00()
	SetObjectiveCompleted(60)
	SetObjectiveDisplayed(65)
EndFunction

Function Fragment_Stage_1010_Item_00()
	PlayerReference().SetValue(Storm_MQ06_AlexChoice, 1.0)
EndFunction

Function Fragment_Stage_1020_Item_00()
	PlayerReference().SetValue(Storm_MQ06_AlexChoice, 2.0)
EndFunction

Function Fragment_Stage_1040_Item_00()
	Actor alex = Alias_Actor_Alex.GetActorReference()
	If alex
		alex.StopCombat()
	EndIf
	If !IsStageDone(1050)
		SetStage(1050)
	EndIf
EndFunction

Function Fragment_Stage_1050_Item_00()
	If !IsStageDone(1100)
		SetStage(1100)
	EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
	SetObjectiveCompleted(65)
	SetObjectiveDisplayed(70)
	EndDisguise()
	MakeCultistsHostile()
EndFunction

Function Fragment_Stage_1110_Item_00()
	SetObjectiveCompleted(70)
	If !IsStageDone(1200)
		SetStage(1200)
	EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
	SetObjectiveCompleted(70)
	SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_1210_Item_00()
	If !Storm_MQ06_HugoPt3_AudreyPathToControlRoom.IsPlaying()
		Storm_MQ06_HugoPt3_AudreyPathToControlRoom.Start()
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(80)
	EndDisguise()
	ObjectReference player = PlayerReference()
	Bool startFinale = False
	Float trackerValue = player.GetValue(Storm_MQ00_QuestProgressionTracker)
	If trackerValue < 2.0
		trackerValue += 1.0
		If trackerValue > 2.0
			trackerValue = 2.0
		EndIf
		player.SetValue(Storm_MQ00_QuestProgressionTracker, trackerValue)
		If trackerValue >= 2.0
			startFinale = True
		EndIf
	EndIf
	player.SetValue(Storm_MQ_AudreyAwayValue, 0.0)
	player.SetValue(Storm_MQ_HugoAwayValue, 0.0)
	If startFinale
		Storm_MQ13_Finale_StartKeyword.SendStoryEvent(None, player, player)
	EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
	EndDisguise()
	PlayerReference().SetValue(Storm_MQ_AudreyAwayValue, 0.0)
	PlayerReference().SetValue(Storm_MQ_HugoAwayValue, 0.0)
EndFunction

; "Has Player Collected Disguise" - set by the player alias' DefaultAliasInventoryManagement
; when Storm_MQ06_Clothes_CultistAdept (ARMO 73D7B6) enters inventory. The stage flag itself
; is the payload; equipping the disguise is what advances the quest, via stage 320.
Function Fragment_Stage_0005_Item_00()
	If IsStageDone(300) && !IsStageDone(320) && !IsObjectiveCompleted(30)
		SetObjectiveDisplayed(30)
	EndIf
EndFunction
