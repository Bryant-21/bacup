Function Fragment_Stage_0010_Item_00()
    EN07_FissureQuestScript controller = (Self as Quest) as EN07_FissureQuestScript
    If controller == None
        Return
    EndIf
    controller.OpenFissure()
    If !GetStageDone(controller.iFissureOpenedStage)
        SetStage(controller.iFissureOpenedStage)
    EndIf
    If !GetStageDone(controller.iStartEnemySpawn)
        SetStage(controller.iStartEnemySpawn)
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    EN07_FissureQuestScript controller = (Self as Quest) as EN07_FissureQuestScript
    If controller != None
        controller.StartFissureWaves()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    EN07_FissureQuestScript controller = (Self as Quest) as EN07_FissureQuestScript
    If controller != None
        controller.CloseFissure()
    EndIf
EndFunction
EndFunction
