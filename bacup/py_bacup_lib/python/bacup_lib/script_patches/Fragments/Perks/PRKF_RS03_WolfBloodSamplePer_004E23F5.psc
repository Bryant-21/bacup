Function SetRS03WolfStage(Actor akPlayer)
    If akPlayer == Game.GetPlayer() && RS03_Inoculation != None && !RS03_Inoculation.IsStageDone(325)
        RS03_Inoculation.SetStage(325)
    EndIf
EndFunction
