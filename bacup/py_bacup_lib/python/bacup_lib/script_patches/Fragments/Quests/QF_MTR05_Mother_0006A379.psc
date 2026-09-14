Function Fragment_Stage_0001_Item_00()
    SetStage(200)
EndFunction

Function Fragment_Stage_0004_Item_00()
    If !IsStageDone(5)
        SetStage(5)
    EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
    SetObjectiveDisplayed(5, True)
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(25, True)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(25, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(50, True)
    SetObjectiveDisplayed(55, True)
    SetObjectiveDisplayed(57, True)
EndFunction

Function Fragment_Stage_0053_Item_00()
    SetObjectiveDisplayed(53, True)
EndFunction

Function Fragment_Stage_0055_Item_00()
    SetObjectiveCompleted(57, True)
    SetObjectiveDisplayed(59, True)
EndFunction

Function Fragment_Stage_0057_Item_00()
    SetObjectiveCompleted(57, True)
    SetObjectiveCompleted(59, True)
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveCompleted(50, True)
    SetObjectiveCompleted(53, True)
    SetObjectiveDisplayed(60, True)
EndFunction

Function Fragment_Stage_0070_Item_00()
    SetObjectiveCompleted(55, True)
    SetObjectiveCompleted(57, True)
    SetObjectiveCompleted(59, True)
    SetObjectiveDisplayed(70, True)
EndFunction

Function Fragment_Stage_0090_Item_00()
    SetObjectiveCompleted(60, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(60, True)
    SetObjectiveCompleted(70, True)
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(150, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(150, True)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0210_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(210, True)
EndFunction

Function Fragment_Stage_0240_Item_00()
    SetObjectiveCompleted(210, True)
    SetObjectiveDisplayed(240, True)
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(240, True)
    SetObjectiveDisplayed(250, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(250, True)
    MTR05_Mother_0300_BeaconDeposited.Start()
EndFunction

Function Fragment_Stage_0305_Item_00()
    If IsStageDone(306)
        If !IsStageDone(307)
            SetStage(307)
        EndIf
    Else
        SetStage(306)
    EndIf
EndFunction

Function Fragment_Stage_0306_Item_00()
    MTR05_Mother_0306_MotherlodeRevealScene.Start()
EndFunction

Function Fragment_Stage_0307_Item_00()
    MTR05_Mother_0307_MotherlodeRevealAltScene.Start()
EndFunction

Function Fragment_Stage_0308_Item_00()
    If !IsStageDone(310)
        SetStage(310)
    EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveDisplayed(310, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(310, True)
    If !IsStageDone(510)
        SetStage(510)
    EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_0999_Item_00()
EndFunction
