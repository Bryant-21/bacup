Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget == None || akTarget != Game.GetPlayer() || NWOT_Nukacade_Points == None || PointsGiven <= 0
        Return
    EndIf
    akTarget.ModValue(NWOT_Nukacade_Points, PointsGiven as Float)
    If PointsGivenMessage != None
        PointsGivenMessage.Show()
    EndIf
    If NWOT_Nukacade_TutorialAV_PrizePoints != None && akTarget.GetValue(NWOT_Nukacade_TutorialAV_PrizePoints) <= 0.0
        akTarget.SetValue(NWOT_Nukacade_TutorialAV_PrizePoints, 1.0)
        If NWOT_Nukacade_Tutorial_PointsMSG != None
            NWOT_Nukacade_Tutorial_PointsMSG.Show()
        EndIf
    EndIf
EndEvent
