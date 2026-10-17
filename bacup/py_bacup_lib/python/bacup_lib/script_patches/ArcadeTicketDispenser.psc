Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    ShowPointsTutorial(akActionRef)
EndEvent

Function ShowPointsTutorial(ObjectReference akPlayer)
    If PointsTutorialSeenAV != None && akPlayer.GetValue(PointsTutorialSeenAV) == 0
        If PointsTutorialMSG != None
            PointsTutorialMSG.Show()
        EndIf
        akPlayer.SetValue(PointsTutorialSeenAV, 1.0)
    EndIf
EndFunction

; PointsCapGlobal (NWOT_Nukacade_TicketCap_Generic, 500) caps one game's payout, not the balance:
; NWOT_Nukacade_Points allows 100000, events grant 7500 and prizes cost up to 20000.
Function PayoutPoints(Actor akPlayer, Int aiScore)
    If akPlayer == None || PointsAV == None || aiScore <= 0
        Return
    EndIf
    Int perTicket = scorePerTicket
    If perTicket <= 0
        perTicket = 1
    EndIf
    Int points = aiScore / perTicket
    If PointsCapGlobal != None && points > PointsCapGlobal.GetValueInt()
        points = PointsCapGlobal.GetValueInt()
    EndIf
    If points <= 0
        Return
    EndIf
    akPlayer.ModValue(PointsAV, points as Float)
    ShowPointsTutorial(akPlayer)
EndFunction
