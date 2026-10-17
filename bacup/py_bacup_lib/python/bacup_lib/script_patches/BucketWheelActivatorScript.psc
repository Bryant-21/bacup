; FO76 Default2StateActivator.SetClosedAnim (snap to the closed pose) has no FO4
; counterpart; FO4's own SetDefaultState restores the closed pose with CloseAnim.
State closed
    Event OnBeginState(String asOldState)
        If Is3DLoaded() && !isWaitingForShutdown
            PlayAnimation(CloseAnim)
        EndIf
    EndEvent
EndState
