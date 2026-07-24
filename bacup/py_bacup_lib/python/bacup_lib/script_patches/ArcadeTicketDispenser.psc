Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If PointsTutorialSeenAV != None && akActionRef.GetValue(PointsTutorialSeenAV) == 0
        If PointsTutorialMSG != None
            PointsTutorialMSG.Show()
        EndIf
        akActionRef.SetValue(PointsTutorialSeenAV, 1.0)
    EndIf
    If PointsAV != None && PointsCapGlobal != None
        Float currentPoints = akActionRef.GetValue(PointsAV)
        Float capValue = PointsCapGlobal.GetValue()
        If currentPoints > capValue
            akActionRef.SetValue(PointsAV, capValue)
        EndIf
    EndIf
EndEvent
