ObjectReference Function PlayerReference()
	Return Alias_Player.GetReference()
EndFunction

Function OpenPumpAccessDoor(ObjectReference accessDoor)
	If accessDoor
		accessDoor.BlockActivation(False)
		accessDoor.RemoveKeyword(BlockPlayerActivation)
		accessDoor.Lock(False)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10)
	PlayerReference().SetValue(Storm_MQ_AudreyAwayValue, 1.0)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0305_Item_00()
	OpenPumpAccessDoor(Alias_PumpAccessDoor2.GetReference())
	ReferenceAlias pumpAccessDoor = Alias_PumpAccessDoor as ReferenceAlias
	If pumpAccessDoor
		OpenPumpAccessDoor(pumpAccessDoor.GetReference())
	EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0320_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0330_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0331_Item_00()
	PlayerReference().SetValue(AV_HitmanLetter, 1.0)
EndFunction

Function Fragment_Stage_0332_Item_00()
	PlayerReference().SetValue(AV_LetterFromAlex, 1.0)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(60)
	SetObjectiveDisplayed(70)
	PlayerReference().SetValue(Storm_MQ_WeatherLabGridAccess, 1.0)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(70)
	SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(80)
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	Storm_MQ05_HugoPt2_StartKeyword.SendStoryEvent(None, PlayerReference(), PlayerReference())
EndFunction

Function Fragment_Stage_0010_Item_00()
	ObjectReference player = PlayerReference()
	If !player
		Return
	EndIf
	If IsStageDone(100) && !IsStageDone(9000)
		player.SetValue(Storm_MQ_AudreyAwayValue, 1.0)
	EndIf
	If IsStageDone(500)
		player.SetValue(Storm_MQ_WeatherLabGridAccess, 1.0)
	EndIf
EndFunction

Function Fragment_Stage_0328_Item_00()
	Actor hugo = Actor_Hugo.GetActorReference()
	If hugo
		hugo.EvaluatePackage()
	EndIf
	If !IsObjectiveCompleted(50)
		SetObjectiveDisplayed(50)
	EndIf
EndFunction
