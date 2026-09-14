Quests:Storm:MQ13:LostGauntletScript Function GauntletController()
    Return (Self as Quest) as Quests:Storm:MQ13:LostGauntletScript
EndFunction

Quests:Storm:MQ13:QuestScript Function FinaleController()
    Return (Self as Quest) as Quests:Storm:MQ13:QuestScript
EndFunction

Function StartSceneIfStopped(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Actor Function PlayerReference()
    Return Alias_Player.GetActorReference()
EndFunction

Function Fragment_Stage_0010_Item_00()
    If IsStageDone(600) && !IsStageDone(700)
        Quests:Storm:MQ13:QuestScript controller = FinaleController()
        If controller != None
            controller.EndBossFight()
        EndIf
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If !IsStageDone(110)
        SetStage(110)
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0120_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(15)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0205_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0210_Item_00()
    SetObjectiveCompleted(25)
    SetObjectiveDisplayed(30)
    StartSceneIfStopped(Scene_FirstGridUnlocked)
    Quests:Storm:MQ13:LostGauntletScript controller = GauntletController()
    If controller != None
        controller.UnlockLaserGrid(0)
    EndIf
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Quests:Storm:MQ13:LostGauntletScript controller = GauntletController()
    If controller != None
        controller.StartGauntletWave(0)
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    StartSceneIfStopped(Scene_SecondGridUnlocked)
    Quests:Storm:MQ13:LostGauntletScript controller = GauntletController()
    If controller != None
        controller.UnlockLaserGrid(1)
        controller.StartGauntletWave(1)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(35)
    StartSceneIfStopped(Scene_ThirdGridUnlocked)
    Quests:Storm:MQ13:LostGauntletScript controller = GauntletController()
    If controller != None
        controller.UnlockAllLaserGrids()
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveCompleted(35)
    StartSceneIfStopped(Scene_ConfrontHugo)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(40)
    Quests:Storm:MQ13:HugoAliasScript hugoController = Alias_Actor_Hugo_Fighting as Quests:Storm:MQ13:HugoAliasScript
    If hugoController != None
        hugoController.BeginFinalFight()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_01()
    Quests:Storm:MQ13:QuestScript controller = FinaleController()
    If controller != None
        controller.BeginBossAddPhase()
    EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
    Quests:Storm:MQ13:QuestScript controller = FinaleController()
    If controller != None
        controller.BeginClonePhase()
    EndIf
EndFunction

Function Fragment_Stage_0520_Item_00()
    Quests:Storm:MQ13:QuestScript controller = FinaleController()
    If controller != None
        controller.BeginFinalSoloPhase()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    Quests:Storm:MQ13:QuestScript controller = FinaleController()
    If controller != None
        controller.EndBossFight()
    EndIf
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0610_Item_00()
    Actor choosingPlayer = PlayerReference()
    If choosingPlayer != None
        choosingPlayer.SetValue(Storm_MQ13_HugoChoice, 1.0)
    EndIf
    SetObjectiveCompleted(50)
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
    Actor choosingPlayer = PlayerReference()
    If choosingPlayer != None
        choosingPlayer.SetValue(Storm_MQ13_HugoChoice, 2.0)
    EndIf
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(70)
    Actor hugo = Alias_Actor_Hugo_Fighting.GetActorReference()
    If hugo != None
        hugo.SetEssential(False)
    EndIf
EndFunction

Function Fragment_Stage_0625_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(71)
    StartSceneIfStopped(Scene_KilledHugo_AfterKill)
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0630_Item_00()
    Actor choosingPlayer = PlayerReference()
    If choosingPlayer != None
        choosingPlayer.SetValue(Storm_MQ13_HugoChoice, 3.0)
    EndIf
    SetObjectiveCompleted(50)
    Actor hugo = Alias_Actor_Hugo_Fighting.GetActorReference()
    If hugo != None
        hugo.StopCombat()
        hugo.SetEssential(True)
    EndIf
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    If IsStageDone(625)
        StartSceneIfStopped(Scene_KilledHugo_ExitDialogue)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_01()
    SetObjectiveDisplayed(90)
    StartSceneIfStopped(Scene_HelpedHugo_ExitDialogue)
EndFunction

Function Fragment_Stage_0700_Item_03()
    SetObjectiveDisplayed(60)
    Quests:Storm:MQ13:QuestScript controller = FinaleController()
    If controller != None
        controller.CaptureHugo()
    EndIf
    StartSceneIfStopped(Scene_CaptureHugo_ExitDialogue)
EndFunction

Function Fragment_Stage_0710_Item_02()
    SetObjectiveCompleted(60)
    SetObjectiveCompleted(71)
    SetObjectiveDisplayed(80)
    If !IsStageDone(750)
        SetStage(750)
    EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
    If IsStageDone(610)
        SetObjectiveDisplayed(90)
    Else
        SetObjectiveDisplayed(80)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveCompleted(90)
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0800_Item_01()
    Actor player = PlayerReference()
    If player != None
        Storm_MQ13_WeatherMachineTriggeredSpell.Cast(player, player)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(100)
EndFunction

Function Fragment_Stage_0900_Item_01()
    ObjectReference hugo = Alias_Actor_Hugo.GetReference()
    If hugo != None
        Alias_Actor_FinalChoice.ForceRefTo(hugo)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_02()
    ObjectReference audrey = Alias_Actor_Audrey_Atrium.GetReference()
    If audrey != None
        Alias_Actor_FinalChoice.ForceRefTo(audrey)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_03()
    ObjectReference oberlin = Alias_Actor_Oberlin.GetReference()
    If oberlin != None
        Alias_Actor_FinalChoice.ForceRefTo(oberlin)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    If !IsStageDone(10000)
        SetStage(10000)
    EndIf
    Stop()
EndFunction

Function Fragment_Stage_10000_Item_00()
    Quests:Storm:MQ13:QuestScript controller = FinaleController()
    If controller != None
        controller.ShutdownFinale()
    EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailAllObjectives()
    Quests:Storm:MQ13:QuestScript controller = FinaleController()
    If controller != None
        controller.ShutdownFinale()
    EndIf
EndFunction
