MTR06_RaceQuestScript Function MTR06_GetController()
    Return (Self as Quest) as MTR06_RaceQuestScript
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(15)
    MTR06_RaceQuestScript controller = MTR06_GetController()
    If controller != None
        controller.MTR06_BeginRace()
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
EndFunction

Function Fragment_Stage_0050_Item_00()
EndFunction

Function Fragment_Stage_0100_Item_00()
    MTR06_RaceQuestScript controller = MTR06_GetController()
    If controller != None
        controller.MTR06_WrapUpRace()
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveCompleted(15)
    MTR06_RaceQuestScript controller = MTR06_GetController()
    If controller != None
        controller.MTR06_CompletePhysicalExam()
        controller.MTR06_ShowSuccess()
    EndIf
    SetStage(150)
EndFunction

Function Fragment_Stage_0120_Item_00()
    SetObjectiveFailed(15)
    MTR06_RaceQuestScript controller = MTR06_GetController()
    If controller != None
        controller.MTR06_ShowFailure()
    EndIf
    SetStage(150)
EndFunction

Function Fragment_Stage_0150_Item_00()
    MTR06_RaceQuestScript controller = MTR06_GetController()
    If controller != None
        controller.MTR06_EndAttempt()
    Else
        Stop()
    EndIf
EndFunction
