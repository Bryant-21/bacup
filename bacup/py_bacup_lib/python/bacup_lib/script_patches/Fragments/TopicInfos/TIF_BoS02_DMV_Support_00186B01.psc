Function Fragment_End(ObjectReference akSpeakerRef)
    If pBoS02_DMVNumber_A3 != None
        pBoS02_DMVNumber_A3.SetValue(1.0)
    EndIf
    If pBoS02_DeptCCooldown != None
        pBoS02_DeptCCooldown.SetValue(1.0)
    EndIf
EndFunction
