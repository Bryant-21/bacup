Function SetRS03GhoulStage(Actor akPlayer)
    If akPlayer == Game.GetPlayer() && RS03_Inoculation != None && !RS03_Inoculation.IsStageDone(315)
        RS03_Inoculation.SetStage(315)
    EndIf
EndFunction
