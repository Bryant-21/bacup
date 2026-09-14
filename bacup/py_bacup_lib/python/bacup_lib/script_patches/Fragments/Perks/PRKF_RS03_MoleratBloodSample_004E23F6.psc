Function SetRS03MoleratStage(Actor akPlayer)
    If akPlayer == Game.GetPlayer() && RS03_Inoculation != None && !RS03_Inoculation.IsStageDone(305)
        RS03_Inoculation.SetStage(305)
    EndIf
EndFunction
