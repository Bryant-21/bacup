Function Fragment_Stage_0100_Item_00()
    SuppliesCount = 0
EndFunction

Function Fragment_Stage_0210_Item_00()
    SuppliesCount += 1
    If SuppliesCount >= 3 && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
    SuppliesCount += 1
    If SuppliesCount >= 3 && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0230_Item_00()
    SuppliesCount += 1
    If SuppliesCount >= 3 && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction
