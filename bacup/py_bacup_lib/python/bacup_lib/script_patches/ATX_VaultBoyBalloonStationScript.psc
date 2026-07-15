; Method fill for the stripped FO76 balloon station. The generated skeleton
; supplies the item list, recharge duration, message, timer ID, and states.

Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    If GetState() == "recharging"
        RechargingMessage.Show()
        Return
    EndIf

    If playerREF != None
        Return
    EndIf

    playerREF = akActionRef as Actor
    If playerREF == None
        Return
    EndIf

    BlockActivation(True, False)
    PlayAnimationAndWait("Play01", "Done")
    playerREF.AddItem(ATX_LL_Misc_BalloonStation_Balloons as Form, 1, False)
    GoToState("recharging")
    StartTimer(RechargeTimer, BalloonCooldownTimerID)
    BlockActivation(False, False)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == BalloonCooldownTimerID
        GoToState("DispenseStopped")
        playerREF = None
    EndIf
EndEvent
