Function Fragment_Stage_0050_Item_00()
    If Scene_Startup_Idles != None && !Scene_Startup_Idles.IsPlaying()
        Scene_Startup_Idles.Start()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
    If Scene_Instructions != None && !Scene_Instructions.IsPlaying()
        Scene_Instructions.Start()
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    If Scene_Intro != None && !Scene_Intro.IsPlaying()
        Scene_Intro.Start()
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    If Scene_Uncle != None && !Scene_Uncle.IsPlaying()
        Scene_Uncle.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
    If Scene_Bells_Louder != None && !Scene_Bells_Louder.IsPlaying()
        Scene_Bells_Louder.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True)
    If Scene_CollectMaterials != None && !Scene_CollectMaterials.IsPlaying()
        Scene_CollectMaterials.Start()
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    If Scene_CollectMaterials_More != None && !Scene_CollectMaterials_More.IsPlaying()
        Scene_CollectMaterials_More.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    If Scene_TheRing != None && !Scene_TheRing.IsPlaying()
        Scene_TheRing.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300, True)
    SetObjectiveDisplayed(400, True)
EndFunction

Function Fragment_Stage_0410_Item_00()
    SetObjectiveCompleted(400, True)
    SetObjectiveDisplayed(410, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(410, True)
    SetObjectiveDisplayed(600, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    ; B21:QuestRewards / CurrencyQuestRewards pay on OnStageSet; let it dispatch before shutdown.
    Utility.Wait(1.0)
    Stop()
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailAllObjectives()
    Stop()
EndFunction
