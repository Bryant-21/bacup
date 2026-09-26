Quests:sfs09:habitatquestscript Function EventScript()
    Quest owner = Self as Quest
    Return owner as Quests:sfs09:habitatquestscript
EndFunction

Bool Function IsEventOver()
    Return IsStageDone(9000) || IsStageDone(9990) || IsStageDone(9991) || IsStageDone(9992)
EndFunction

Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(5)
    ResetEventObjective(10)
    ResetEventObjective(11)
    ResetEventObjective(12)
    ResetEventObjective(13)
    ResetEventObjective(14)
    ResetEventObjective(15)
    ResetEventObjective(16)
    ResetEventObjective(18)
    ResetEventObjective(19)
    ResetEventObjective(20)
    ResetEventObjective(30)
    ResetEventObjective(40)
    ResetEventObjective(50)
    ResetEventObjective(60)
    ResetEventObjective(62)
    ResetEventObjective(64)
    ResetEventObjective(66)
    ResetEventObjective(70)
    ResetEventObjective(110)
    ResetEventObjective(120)
    ResetEventObjective(130)
EndFunction

Function CompleteObjectiveIfOpen(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FailObjectiveIfOpen(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function HideObjectiveIfOpen(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveDisplayed(aiObjective, False)
    EndIf
EndFunction

Function CloseEventObjective(Int aiObjective, Bool abFailed)
    If abFailed
        FailObjectiveIfOpen(aiObjective)
    Else
        CompleteObjectiveIfOpen(aiObjective)
    EndIf
EndFunction

; Closing every open objective also stops the objective timers that would otherwise set later stages.
Function CloseEventObjectives(Bool abFailed)
    CloseEventObjective(5, abFailed)
    CloseEventObjective(10, abFailed)
    CloseEventObjective(11, abFailed)
    CloseEventObjective(12, abFailed)
    CloseEventObjective(13, abFailed)
    CloseEventObjective(14, abFailed)
    CloseEventObjective(15, abFailed)
    CloseEventObjective(16, abFailed)
    CloseEventObjective(18, abFailed)
    CloseEventObjective(19, abFailed)
    CloseEventObjective(20, abFailed)
    CloseEventObjective(30, abFailed)
    CloseEventObjective(40, abFailed)
    CloseEventObjective(50, abFailed)
    CloseEventObjective(60, abFailed)
    CloseEventObjective(62, abFailed)
    CloseEventObjective(64, abFailed)
    CloseEventObjective(66, abFailed)
    CloseEventObjective(70, abFailed)
    HideObjectiveIfOpen(110)
    HideObjectiveIfOpen(120)
    HideObjectiveIfOpen(130)
EndFunction

Function StartScene(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function ShowEventMessage(Message akMessage)
    If akMessage != None
        akMessage.Show()
    EndIf
EndFunction

Function MarkTroughFull(Int aiCountObjective, Int aiNextRankObjective, Int aiFullObjective)
    If IsEventOver() || IsStageDone(200)
        Return
    EndIf
    HideObjectiveIfOpen(aiCountObjective)
    HideObjectiveIfOpen(aiNextRankObjective)
    SetObjectiveDisplayed(aiFullObjective, True)
EndFunction

Function StartEndlessWave(Int aiHabitat, Int aiRoundEndStage)
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.StartEndlessWave(aiHabitat, aiRoundEndStage)
    EndIf
EndFunction

Function StartAlphaPhase(Int aiHabitat, Message akHabitatMessage)
    If IsStageDone(9000) || IsEventOver()
        Return
    EndIf
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript == None || !eventScript.BeginAlphaWave(aiHabitat)
        Return
    EndIf
    SetObjectiveDisplayed(60, True, True)
    If akHabitatMessage != None
        ShowEventMessage(akHabitatMessage)
    Else
        ShowEventMessage(SFS09_Habitat_Message_Alpha)
    EndIf
EndFunction

Function FriendlyCreatureDied(Int aiHabitat, Int aiCreatureObjective)
    If IsEventOver()
        Return
    EndIf
    FailObjectiveIfOpen(aiCreatureObjective)
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.FriendlyCreatureDied(aiHabitat)
    EndIf
EndFunction

Function FinishEventRun(Bool abSucceeded)
    CloseEventObjectives(!abSucceeded)
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.FinishEvent()
    EndIf
EndFunction

; Only the first failure stage ends the run, and never after the event succeeded.
Bool Function FailEvent(Int aiStage)
    If IsStageDone(9000)
        Return False
    EndIf
    Int failStage = 9990
    While failStage <= 9992
        If failStage != aiStage && IsStageDone(failStage)
            Return False
        EndIf
        failStage += 1
    EndWhile
    FinishEventRun(False)
    Return True
EndFunction

; RunOnStart: ARIC-4 asks the players to initialize the experiment at the mainframe terminal.
Function Fragment_Stage_0100_Item_00()
    ResetEventObjectives()
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.ResetEvent()
    EndIf
    SetObjectiveDisplayed(5, True, True)
    StartScene(Scene_Intro)
EndFunction

Function Fragment_Stage_0150_Item_00()
    If IsEventOver()
        Return
    EndIf
    CompleteObjectiveIfOpen(5)
    SetObjectiveDisplayed(10, True, True)
    SetObjectiveDisplayed(11, True)
    SetObjectiveDisplayed(13, True)
    SetObjectiveDisplayed(15, True)
    SetObjectiveDisplayed(110, True)
    SetObjectiveDisplayed(120, True)
    SetObjectiveDisplayed(130, True)
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.BeginTroughPhase()
    EndIf
    ShowEventMessage(SFS09_Habitat_Message_Phase1)
    StartScene(Scene_Instructions)
EndFunction

Function Fragment_Stage_0170_Item_00()
    If IsEventOver() || IsStageDone(200)
        Return
    EndIf
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.ShutDownMainframe()
    EndIf
    StartScene(Scene_Takeover)
EndFunction

Function Fragment_Stage_0180_Item_00()
    MarkTroughFull(11, 110, 12)
EndFunction

Function Fragment_Stage_0182_Item_00()
    MarkTroughFull(15, 130, 16)
EndFunction

Function Fragment_Stage_0184_Item_00()
    MarkTroughFull(13, 120, 14)
EndFunction

Function Fragment_Stage_0190_Item_00()
    If IsEventOver() || IsStageDone(200)
        Return
    EndIf
    SetObjectiveDisplayed(18, True, True)
    ShowEventMessage(SFS09_Habitat_Message_FinalCall)
    StartScene(Scene_LastCall)
EndFunction

Function Fragment_Stage_0200_Item_00()
    If IsEventOver()
        Return
    EndIf
    CompleteObjectiveIfOpen(10)
    CompleteObjectiveIfOpen(18)
    CompleteObjectiveIfOpen(11)
    CompleteObjectiveIfOpen(12)
    CompleteObjectiveIfOpen(13)
    CompleteObjectiveIfOpen(14)
    CompleteObjectiveIfOpen(15)
    CompleteObjectiveIfOpen(16)
    HideObjectiveIfOpen(110)
    HideObjectiveIfOpen(120)
    HideObjectiveIfOpen(130)
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.EndTroughPhase()
    EndIf
    SetObjectiveDisplayed(19, True, True)
    SetObjectiveDisplayed(62, True)
    SetObjectiveDisplayed(64, True)
    SetObjectiveDisplayed(66, True)
    SetObjectiveDisplayed(70, True)
    ShowEventMessage(SFS09_Habitat_Message_Phase2)
EndFunction

Function Fragment_Stage_0300_Item_00()
    If IsEventOver()
        Return
    EndIf
    CompleteObjectiveIfOpen(19)
    SetObjectiveDisplayed(20, True, True)
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.StartHabitatWaves(1)
    EndIf
    ShowEventMessage(SFS09_Habitat_Message_Wave1)
    StartScene(Scene_Wave1)
EndFunction

Function Fragment_Stage_0310_Item_00()
    StartEndlessWave(0, 350)
EndFunction

Function Fragment_Stage_0320_Item_00()
    StartEndlessWave(1, 350)
EndFunction

Function Fragment_Stage_0330_Item_00()
    StartEndlessWave(2, 350)
EndFunction

Function Fragment_Stage_0350_Item_00()
    If IsEventOver()
        Return
    EndIf
    CompleteObjectiveIfOpen(20)
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.EndWaveRound()
    EndIf
    SetObjectiveDisplayed(30, True, True)
    StartScene(Scene_Wave1Complete)
EndFunction

Function Fragment_Stage_0400_Item_00()
    If IsEventOver()
        Return
    EndIf
    CompleteObjectiveIfOpen(30)
    SetObjectiveDisplayed(40, True, True)
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.StartHabitatWaves(2)
    EndIf
    ShowEventMessage(SFS09_Habitat_Message_Wave2)
EndFunction

Function Fragment_Stage_0410_Item_00()
    StartEndlessWave(0, 450)
EndFunction

Function Fragment_Stage_0420_Item_00()
    StartEndlessWave(1, 450)
EndFunction

Function Fragment_Stage_0430_Item_00()
    StartEndlessWave(2, 450)
EndFunction

Function Fragment_Stage_0450_Item_00()
    If IsEventOver()
        Return
    EndIf
    CompleteObjectiveIfOpen(40)
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.EndWaveRound()
    EndIf
    SetObjectiveDisplayed(50, True, True)
    StartScene(Scene_Wave2Complete)
EndFunction

Function Fragment_Stage_0500_Item_00()
    If IsEventOver()
        Return
    EndIf
    CompleteObjectiveIfOpen(50)
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.ChooseAlphaHabitat()
    EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
    StartAlphaPhase(0, SFS09_Habitat_Message_Alpha_A)
EndFunction

Function Fragment_Stage_0520_Item_00()
    StartAlphaPhase(1, SFS09_Habitat_Message_Alpha_C)
EndFunction

Function Fragment_Stage_0530_Item_00()
    StartAlphaPhase(2, SFS09_Habitat_Message_Alpha_B)
EndFunction

Function Fragment_Stage_0910_Item_00()
    FriendlyCreatureDied(0, 62)
EndFunction

Function Fragment_Stage_0920_Item_00()
    FriendlyCreatureDied(1, 66)
EndFunction

Function Fragment_Stage_0930_Item_00()
    FriendlyCreatureDied(2, 64)
EndFunction

; Success: the alpha fell or the final wave's objective timer ran out with a friendly creature alive.
Function Fragment_Stage_9000_Item_00()
    If IsStageDone(9990) || IsStageDone(9991) || IsStageDone(9992)
        Return
    EndIf
    FinishEventRun(True)
    If Scene_QuestComplete != None && !Scene_QuestComplete.IsPlaying()
        Scene_QuestComplete.Start()
    EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
    If FailEvent(9990) && Scene_QuestFail != None && !Scene_QuestFail.IsPlaying()
        Scene_QuestFail.Start()
    EndIf
EndFunction

Function Fragment_Stage_9991_Item_00()
    If FailEvent(9991) && Scene_QuestFail != None && !Scene_QuestFail.IsPlaying()
        Scene_QuestFail.Start()
    EndIf
EndFunction

Function Fragment_Stage_9992_Item_00()
    If FailEvent(9992) && Scene_QuestFail != None && !Scene_QuestFail.IsPlaying()
        Scene_QuestFail.Start()
    EndIf
EndFunction

; Set when the ending scene completes, or by the quest script's failsafe timer.
Function Fragment_Stage_10000_Item_00()
    Quests:sfs09:habitatquestscript eventScript = EventScript()
    If eventScript != None
        eventScript.CleanUpEvent()
    EndIf
    Stop()
EndFunction
