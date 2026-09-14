Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || MTR02_Miner.IsStageDone(255)
        Return
    EndIf

    If !MTR02_Miner.IsStageDone(120)
        MTR02_MinerRegFailure.Show()
        Return
    EndIf

    Actor player = MTR02_MinerPlayer.GetReference() as Actor
    If player != None && player.WornHasKeyword(MTR02_MinerBuiltLeftArm) && player.WornHasKeyword(MTR02_MinerBuiltRightArm) && player.WornHasKeyword(MTR02_MinerBuiltHelmet) && player.WornHasKeyword(MTR02_MinerBuiltTorso) && player.WornHasKeyword(MTR02_MinerBuiltLeftLeg) && player.WornHasKeyword(MTR02_MinerBuiltRightLeg)
        MTR02_Miner.SetStage(255)
        MTR02_MinerRegSuccess.Show()
    Else
        MTR02_MinerRegFailure.Show()
    EndIf
EndEvent
