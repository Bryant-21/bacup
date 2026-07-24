Function Fragment_Stage_0200_Item_00()
    If W05_Tutorial_LegendaryScrip != None
        W05_Tutorial_LegendaryScrip.Show()
    EndIf
    SetObjectiveDisplayed(200, True, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(200, True)
    Stop()
EndFunction
