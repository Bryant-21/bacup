Function Fragment_Stage_0200_Item_00()
    If MTR11_Panel01_Scene != None && !MTR11_Panel01_Scene.IsPlaying()
        MTR11_Panel01_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    If MTR11_Panel02_Scene != None && !MTR11_Panel02_Scene.IsPlaying()
        MTR11_Panel02_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    If MTR11_Panel03_Scene != None && !MTR11_Panel03_Scene.IsPlaying()
        MTR11_Panel03_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_9998_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9999_Item_00()
    CompleteAllObjectives()
    ; B21:QuestRewards / CurrencyQuestRewards pay on OnStageSet; let it dispatch before shutdown.
    Utility.Wait(1.0)
    Stop()
EndFunction
