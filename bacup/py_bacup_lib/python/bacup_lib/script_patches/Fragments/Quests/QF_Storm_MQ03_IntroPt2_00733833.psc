Function StartSceneIfStopped(Scene sceneToStart)
	If sceneToStart != None && !sceneToStart.IsPlaying()
		sceneToStart.Start()
	EndIf
EndFunction

Function AdvancePastOnlineEncounter()
	If !IsStageDone(400)
		SetStage(400)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10)
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None
		If AV_AudreyAwayValue != None
			playerRef.SetValue(AV_AudreyAwayValue, 1.0)
		EndIf
		If Storm_MQ_HildaAwayValue != None
			playerRef.SetValue(Storm_MQ_HildaAwayValue, 1.0)
		EndIf
	EndIf
	ObjectReference deadBody = Alias_DeadBody.GetReference()
	If deadBody != None && deadBody.IsDisabled()
		deadBody.Enable()
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(15)
	AdvancePastOnlineEncounter()
EndFunction

Function Fragment_Stage_0300_Item_00()
	AdvancePastOnlineEncounter()
EndFunction

Function Fragment_Stage_0310_Item_00()
	AdvancePastOnlineEncounter()
EndFunction

Function Fragment_Stage_0320_Item_00()
	AdvancePastOnlineEncounter()
EndFunction

Function Fragment_Stage_0330_Item_00()
	AdvancePastOnlineEncounter()
EndFunction

Function Fragment_Stage_0340_Item_00()
	AdvancePastOnlineEncounter()
EndFunction

Function Fragment_Stage_0350_Item_00()
	AdvancePastOnlineEncounter()
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(15)
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0500_Item_00()
	StartSceneIfStopped(Scene_Hilda_Intercom)
EndFunction

Function Fragment_Stage_0550_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(50)
	ObjectReference deadBody = Alias_DeadBody.GetReference()
	If deadBody != None && Key_LostHand != None && deadBody.GetItemCount(Key_LostHand) == 0
		deadBody.AddItem(Key_LostHand, 1, True)
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(60)
	SetObjectiveDisplayed(70)
	ObjectReference wallDoor = Alias_DHM_WallDoor.GetReference()
	If wallDoor != None
		wallDoor.SetOpen(True)
	EndIf
	ObjectReference elevatorDoor = Alias_DHM_EDoor.GetReference()
	If elevatorDoor != None
		elevatorDoor.SetOpen(True)
	EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
	SetObjectiveCompleted(70)
	SetObjectiveDisplayed(80)
	ObjectReference vaultDoor = Alias_V63E_EDoor.GetReference()
	If vaultDoor != None
		vaultDoor.SetOpen(True)
	EndIf
EndFunction

Function Fragment_Stage_0950_Item_00()
	StartSceneIfStopped(Scene_Hilda_Meeting)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(80)
	SetObjectiveDisplayed(90)
	If Scene_Hilda_Meeting != None && Scene_Hilda_Meeting.IsPlaying()
		Scene_Hilda_Meeting.Stop()
	EndIf
	ObjectReference vaultDoor = Alias_V63E_EDoor.GetReference()
	If vaultDoor != None
		vaultDoor.SetOpen(True)
	EndIf
EndFunction

Function Fragment_Stage_1050_Item_00()
	ObjectReference vaultDoor = Alias_V63E_EDoor.GetReference()
	If vaultDoor != None
		vaultDoor.SetOpen(False)
	EndIf
	ObjectReference hildaRef = Actor_Hilda.GetReference()
	If hildaRef != None && !hildaRef.IsDisabled()
		hildaRef.Disable()
	EndIf
EndFunction

Function Fragment_Stage_1060_Item_00()
	StartSceneIfStopped(Scene_Hugo_Intro)
EndFunction

Function Fragment_Stage_1100_Item_00()
	SetObjectiveCompleted(90)
	SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_1130_Item_00()
	SetObjectiveCompleted(100)
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && Holotape_OberlinReport != None && playerRef.GetItemCount(Holotape_OberlinReport) == 0
		playerRef.AddItem(Holotape_OberlinReport, 1, False)
	EndIf
	If !IsStageDone(1200)
		SetStage(1200)
	EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
	SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_1200_Item_01()
	SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_1300_Item_00()
	SetObjectiveCompleted(110)
	CompleteAllObjectives()
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	Storm_MQ04_HugoPt1_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	Storm_MQ07_OberlinPt1_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	If !IsStageDone(9500)
		SetStage(9500)
	EndIf
EndFunction

Function Fragment_Stage_9500_Item_00()
	Stop()
EndFunction
