; Quests:E07_Mothman:QuestScript owns pacing and counters; these fragments own objectives, messages and scenes.
; Clarence's speech line sets 150 itself; objective 530 covers both the waterside and the final wave.
Quests:E07_Mothman:QuestScript Function Controller()
    Return (Self as Quest) as Quests:E07_Mothman:QuestScript
EndFunction

Function StartSceneIfStopped(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function ShowMessage(Message akMessage)
    If akMessage != None
        akMessage.Show()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Quests:E07_Mothman:QuestScript ctrl = Controller()
    If ctrl != None
        ctrl.StartEvent()
    EndIf
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0160_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0240_Item_00()
    SetObjectiveCompleted(30, True)
    StartSceneIfStopped(E07_Mothman_BeginTask)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(100, True)
    StartSceneIfStopped(E07_Mothman_Church_BeginTask)
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveCompleted(100, True)
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveDisplayed(200, True)
    StartSceneIfStopped(E07_Mothman_Bridge_BeginTask)
EndFunction

Function Fragment_Stage_2500_Item_00()
    SetObjectiveCompleted(200, True)
EndFunction

Function Fragment_Stage_3000_Item_00()
    SetObjectiveDisplayed(300, True)
    StartSceneIfStopped(E07_Mothman_Waterside_BeginTask)
EndFunction

Function Fragment_Stage_3500_Item_00()
    SetObjectiveCompleted(300, True)
EndFunction

Function Fragment_Stage_4000_Item_00()
    SetObjectiveDisplayed(400, True)
    StartSceneIfStopped(E07_Mothman_TasksComplete)
EndFunction

Function Fragment_Stage_4500_Item_00()
    SetObjectiveCompleted(400, True)
    If E07A_Spell_IMOD_MothDust != None
        E07A_Spell_IMOD_MothDust.Cast(Game.GetPlayer(), Game.GetPlayer())
    EndIf
    StartSceneIfStopped(E07A_Mothman_WeatherStarted)
EndFunction

Function Fragment_Stage_4600_Item_00()
    SetObjectiveDisplayed(500, True)
    SetObjectiveDisplayed(901, True)
    SetObjectiveDisplayed(902, True)
    SetObjectiveDisplayed(903, True)
    StartSceneIfStopped(E07_Mothman_Ritual_Combat)
EndFunction

Function Fragment_Stage_5040_Item_00()
    ShowMessage(E07_Mothman_Message_Defense_Church_Prep)
    SetObjectiveDisplayed(505, True)
EndFunction

Function Fragment_Stage_5050_Item_00()
    SetStage(5055)
EndFunction

Function Fragment_Stage_5055_Item_00()
    ShowMessage(E07_Mothman_Message_Defense_Church_Start)
    SetObjectiveCompleted(505, True)
    SetObjectiveDisplayed(510, True)
EndFunction

Function Fragment_Stage_5100_Item_00()
    SetObjectiveCompleted(510, True)
    ShowMessage(E07_Mothman_Message_Defense_Bridge_Prep)
    SetObjectiveDisplayed(515, True)
EndFunction

Function Fragment_Stage_5150_Item_00()
    SetStage(5155)
EndFunction

Function Fragment_Stage_5155_Item_00()
    ShowMessage(E07_Mothman_Message_Defense_Bridge_Start)
    SetObjectiveCompleted(515, True)
    SetObjectiveDisplayed(520, True)
EndFunction

Function Fragment_Stage_5200_Item_00()
    SetObjectiveCompleted(520, True)
    ShowMessage(E07_Mothman_Message_Defense_Waterside_Prep)
    SetObjectiveDisplayed(525, True)
EndFunction

Function Fragment_Stage_5250_Item_00()
    SetStage(5251)
    SetStage(5255)
EndFunction

Function Fragment_Stage_5255_Item_00()
    ShowMessage(E07_Mothman_Message_Defense_Waterside_Start)
    SetObjectiveCompleted(525, True)
    SetObjectiveDisplayed(530, True)
EndFunction

Function Fragment_Stage_5300_Item_00()
    ShowMessage(E07_Mothman_Message_Defense_Final_Prep)
EndFunction

Function Fragment_Stage_5350_Item_00()
    SetStage(5355)
EndFunction

Function Fragment_Stage_5355_Item_00()
    ShowMessage(E07_Mothman_Message_Defense_Final_Start)
EndFunction

Function Fragment_Stage_5400_Item_00()
    SetObjectiveCompleted(530, True)
    SetObjectiveCompleted(500, True)
    SetObjectiveDisplayed(901, False)
    SetObjectiveDisplayed(902, False)
    SetObjectiveDisplayed(903, False)
    SetStage(5450)
EndFunction

Function Fragment_Stage_5450_Item_00()
    ShowMessage(E07_Mothman_Message_Return_Dance)
    SetStage(5500)
EndFunction

Function Fragment_Stage_5500_Item_00()
    SetStage(6000)
EndFunction

Function Fragment_Stage_6000_Item_00()
    SetObjectiveDisplayed(600, True)
EndFunction

Function Fragment_Stage_6500_Item_00()
    SetObjectiveCompleted(600, True)
    If E07A_Mothman_CanCommune_AV != None
        Game.GetPlayer().SetValue(E07A_Mothman_CanCommune_AV, 1.0)
    EndIf
EndFunction

; Ritual_Complete's completion sets 9999.
Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    StartSceneIfStopped(E07_Mothman_Ritual_Complete)
EndFunction

Function Fragment_Stage_9100_Item_00()
    SetObjectiveFailed(901, True)
EndFunction

Function Fragment_Stage_9200_Item_00()
    SetObjectiveFailed(902, True)
EndFunction

Function Fragment_Stage_9300_Item_00()
    SetObjectiveFailed(903, True)
EndFunction

Function Fragment_Stage_9996_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9997_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9998_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9999_Item_00()
    If E07A_Mothman_CanCommune_AV != None
        Game.GetPlayer().SetValue(E07A_Mothman_CanCommune_AV, 0.0)
    EndIf
    Stop()
EndFunction
