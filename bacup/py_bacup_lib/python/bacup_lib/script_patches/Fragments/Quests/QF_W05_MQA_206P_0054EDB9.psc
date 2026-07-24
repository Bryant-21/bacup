; TODO

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0150_Item_00()
    If W05_MQA_206P_Greet && !W05_MQA_206P_Greet.IsPlaying()
        W05_MQA_206P_Greet.Start()
    EndIf
EndFunction

Function Fragment_Stage_0105_Item_00()
    SetObjectiveDisplayed(105)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
    Default2StateActivator louDoor = Alias_LouDoor.GetReference() as Default2StateActivator
    If louDoor
        louDoor.SetActivatorOpen(True)
    EndIf
    If !IsStageDone(250)
        SetStage(250)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveDisplayed(250)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveDisplayed(450)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0525_Item_00()
    SetObjectiveDisplayed(525)
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveDisplayed(550)
EndFunction

Function Fragment_Stage_0575_Item_00()
    SetObjectiveDisplayed(575)
EndFunction

Function Fragment_Stage_0585_Item_00()
    If W05_MQA_206P_LiveOrDie && !W05_MQA_206P_LiveOrDie.IsPlaying()
        W05_MQA_206P_LiveOrDie.Start()
    EndIf
EndFunction

Function Fragment_Stage_0590_Item_00()
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
    If W05_MQA_206P_OperationsScene && !W05_MQA_206P_OperationsScene.IsPlaying()
        W05_MQA_206P_OperationsScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(800)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_5000_Item_00()
    If W05_MQA_206P_Raiders_Leaving && !W05_MQA_206P_Raiders_Leaving.IsPlaying()
        W05_MQA_206P_Raiders_Leaving.Start()
    EndIf
EndFunction

Function Fragment_Stage_5050_Item_00()
    If W05_MQA_206P_Johnny_001_Gold && !W05_MQA_206P_Johnny_001_Gold.IsPlaying()
        W05_MQA_206P_Johnny_001_Gold.Start()
    EndIf
EndFunction

Function Fragment_Stage_5200_Item_00()
    SetObjectiveDisplayed(5200)
EndFunction

Function Fragment_Stage_5220_Item_00()
    SetObjectiveDisplayed(5220)
EndFunction

Function Fragment_Stage_9999_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_0030_Item_00()
    If W05_MQA_206P_SeeChase && !W05_MQA_206P_SeeChase.IsPlaying()
        W05_MQA_206P_SeeChase.Start()
    EndIf
EndFunction

Function Fragment_Stage_0033_Item_00()
    If W05_MQA_206P_Ghoul && !W05_MQA_206P_Ghoul.IsPlaying()
        W05_MQA_206P_Ghoul.Start()
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    If W05_MQA_206P_GoldRoom && !W05_MQA_206P_GoldRoom.IsPlaying()
        W05_MQA_206P_GoldRoom.Start()
    EndIf
EndFunction
