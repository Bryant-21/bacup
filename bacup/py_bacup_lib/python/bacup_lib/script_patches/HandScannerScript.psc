State Initial
    ; FO76 gated this on Default2StateActivator.IsSynced — a network flag meaning
    ; "the server has pushed this activator's state to us yet". FO4's
    ; Default2StateActivator has no such property (the read aborts the call and
    ; the scanner never picks a state), and there is no server: IsOpen is
    ; authoritative the moment the ref resolves. So the gate is always true, and
    ; the unsynced startsgreen/startsblue branch is unreachable. Those states are
    ; identical to Green/Blue anyway except for animation guards that only apply
    ; when arriving from a transition state, which cannot happen from Initial.
    Function SetNextState()
        If mySyncActivator == None
            Self.GoToState("startsblue")
        ElseIf mySyncActivator.IsOpen
            Self.GoToState("Green")
        Else
            Self.GoToState("Blue")
        EndIf
    EndFunction

    Function Init_SetMySyncActivator()
        Bool foundMultiple2StateActivators = False
        ObjectReference[] refs
        Int i = 0
        default2stateactivator current

        If shouldSyncWithActivator
            refs = Self.GetLinkedRefChain(LinkedRefToActivate, 100)

            While !foundMultiple2StateActivators && i < refs.Length
                current = refs[i] as default2stateactivator
                If current != None
                    If mySyncActivator == None
                        mySyncActivator = current
                    Else
                        foundMultiple2StateActivators = True
                    EndIf
                EndIf
                i += 1
            EndWhile

            If foundMultiple2StateActivators
                mySyncActivator = None
            EndIf
        EndIf
    EndFunction
EndState
