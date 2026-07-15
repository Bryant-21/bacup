; Method fill for the partially stripped FO76 fortune teller. The generated
; skeleton supplies the reward list, spell, dispense delay, and timer ID.

Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    If activatingPlayer != None
        Return
    EndIf

    activatingPlayer = akActionRef as Actor
    If activatingPlayer == None
        Return
    EndIf

    BlockActivation(True, False)
    PlayAnimation("Play01")
    StartTimer(TimeToDispense, FortuneGrantedTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == FortuneGrantedTimerID
        If activatingPlayer != None
            activatingPlayer.AddItem(pFortuneBooks as Form, 1, False)
            SpellToCast.Cast(Self, activatingPlayer)
        EndIf
        activatingPlayer = None
        BlockActivation(False, False)
    EndIf
EndEvent
