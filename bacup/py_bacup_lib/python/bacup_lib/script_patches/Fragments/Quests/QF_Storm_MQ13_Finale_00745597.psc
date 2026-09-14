Quests:Storm:MQ13:Part1QuestScript Function FinaleController()
    Return (Self as Quest) as Quests:Storm:MQ13:Part1QuestScript
EndFunction

Function StartSceneIfStopped(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function SetHarvesterActive(ReferenceAlias akHarvesterAlias, Bool abActive)
    ObjectReference harvester = akHarvesterAlias.GetReference()
    If harvester != None
        If abActive
            harvester.SetValue(Storm_MQ13_HarvesterActive, 1.0)
        Else
            harvester.SetValue(Storm_MQ13_HarvesterActive, 0.0)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0000_Item_00()
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0115_Item_00()
    StartSceneIfStopped(Storm_MQ13_Finale_DiscussingPlayerActions)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0210_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(35)
    SetHarvesterActive(Alias_Ref_LightningHarvesterA, True)
    StartSceneIfStopped(Storm_MQ13_Finale_HarvesterA_Activated)
    Quests:Storm:MQ13:Part1QuestScript controller = FinaleController()
    If controller != None
        controller.StartHarvesterDefense(0, 210, 220)
    EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
    SetHarvesterActive(Alias_Ref_LightningHarvesterA, False)
    SetObjectiveCompleted(35)
    StartSceneIfStopped(Storm_MQ13_Finale_HarvesterA_Completed)
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0225_Item_00()
    SetHarvesterActive(Alias_Ref_LightningHarvesterA, False)
    Quests:Storm:MQ13:Part1QuestScript controller = FinaleController()
    If controller != None
        controller.FinishHarvesterDefense(210, 220)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(45)
    SetHarvesterActive(Alias_Ref_LightningHarvesterB, True)
    StartSceneIfStopped(Storm_MQ13_Finale_HarvesterB_Activated)
    Quests:Storm:MQ13:Part1QuestScript controller = FinaleController()
    If controller != None
        controller.StartHarvesterDefense(1, 310, 320)
    EndIf
EndFunction

Function Fragment_Stage_0320_Item_00()
    SetHarvesterActive(Alias_Ref_LightningHarvesterB, False)
    SetObjectiveCompleted(45)
    StartSceneIfStopped(Storm_MQ13_Finale_HarvesterB_Completed)
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0325_Item_00()
    SetHarvesterActive(Alias_Ref_LightningHarvesterB, False)
    Quests:Storm:MQ13:Part1QuestScript controller = FinaleController()
    If controller != None
        controller.FinishHarvesterDefense(310, 320)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0410_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(55)
    SetHarvesterActive(Alias_Ref_LightningHarvesterC, True)
    StartSceneIfStopped(Storm_MQ13_Finale_HarvesterC_Activated)
    Quests:Storm:MQ13:Part1QuestScript controller = FinaleController()
    If controller != None
        controller.StartHarvesterDefense(2, 410, 420)
    EndIf
EndFunction

Function Fragment_Stage_0420_Item_00()
    SetHarvesterActive(Alias_Ref_LightningHarvesterC, False)
    SetObjectiveCompleted(55)
    SetObjectiveDisplayed(60)
    StartSceneIfStopped(Storm_MQ13_Finale_HarvesterC_Completed)
    If !IsStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0425_Item_00()
    SetHarvesterActive(Alias_Ref_LightningHarvesterC, False)
    Quests:Storm:MQ13:Part1QuestScript controller = FinaleController()
    If controller != None
        controller.FinishHarvesterDefense(410, 420)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(60)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Storm_MQ13_Finale_Pt2_StartKeyword.SendStoryEvent()
EndFunction

Function Fragment_Stage_10000_Item_00()
    Quests:Storm:MQ13:Part1QuestScript controller = FinaleController()
    If controller != None
        controller.CancelTimer(220)
        controller.CancelTimer(320)
        controller.CancelTimer(420)
    EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailAllObjectives()
    SetHarvesterActive(Alias_Ref_LightningHarvesterA, False)
    SetHarvesterActive(Alias_Ref_LightningHarvesterB, False)
    SetHarvesterActive(Alias_Ref_LightningHarvesterC, False)
    Quests:Storm:MQ13:Part1QuestScript controller = FinaleController()
    If controller != None
        controller.CancelTimer(220)
        controller.CancelTimer(320)
        controller.CancelTimer(420)
    EndIf
EndFunction
