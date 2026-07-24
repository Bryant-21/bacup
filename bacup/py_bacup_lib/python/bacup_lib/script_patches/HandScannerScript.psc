State Initial
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
