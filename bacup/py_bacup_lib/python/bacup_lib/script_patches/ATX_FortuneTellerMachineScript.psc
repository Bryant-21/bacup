; OnActivateClients was raised on FO76 clients when any player activated this ref.
; The OnActivate handler below already plays "Play01" locally.
; @drop-member OnActivateClients

State fortunestarted
	Event OnActivate(ObjectReference akActionRef)
	EndEvent

	Event OnTimer(Int aiTimerID)
		FinishFortune(aiTimerID)
	EndEvent
EndState

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
    GoToState("fortunestarted")
    PlayAnimation("Play01")
    StartTimer(TimeToDispense, FortuneGrantedTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    FinishFortune(aiTimerID)
EndEvent

Function FinishFortune(Int aiTimerID)
    If aiTimerID == FortuneGrantedTimerID
        If activatingPlayer != None
            activatingPlayer.AddItem(pFortuneBooks as Form, 1, False)
            activatingPlayer.DispelSpell(SpellToCast)
            SpellToCast.Cast(activatingPlayer, activatingPlayer)
        EndIf
        activatingPlayer = None
        BlockActivation(False, False)
        GoToState("FortuneStopped")
    EndIf
EndFunction
