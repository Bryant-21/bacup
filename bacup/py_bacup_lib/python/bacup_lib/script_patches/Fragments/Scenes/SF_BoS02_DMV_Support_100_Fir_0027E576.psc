Function Fragment_Begin()
    If pBoS02_DeptBCooldown != None
        pBoS02_DeptBCooldown.SetValue(0.0)
    EndIf
EndFunction

Function Fragment_End()
    If pBoS02_DMVNumber_42 != None
        pBoS02_DMVNumber_42.SetValue(1.0)
    EndIf
    If pBoS02_DeptBCooldown != None
        pBoS02_DeptBCooldown.SetValue(1.0)
    EndIf
EndFunction
