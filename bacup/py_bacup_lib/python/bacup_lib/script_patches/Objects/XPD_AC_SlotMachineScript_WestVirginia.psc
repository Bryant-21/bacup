Event OnActivate(ObjectReference akActionRef)
    Actor activatingActor = akActionRef as Actor
    If activatingActor != Game.GetPlayer() || ATX_BuffLuck == None
        Return
    EndIf
    If !IsPowered()
        If MessageNoPower != None
            MessageNoPower.Show()
        EndIf
        Return
    EndIf

    activatingActor.DispelSpell(ATX_BuffLuck)
    ATX_BuffLuck.Cast(activatingActor, activatingActor)
EndEvent
