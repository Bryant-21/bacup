Event OnContainerChanged(ObjectReference akNewContainer, ObjectReference akOldContainer)
    Actor playerRef = Game.GetPlayer()
    If playerRef && AVToSet && akNewContainer == playerRef
        If playerRef.GetValue(AVToSet) != SetToValue
            playerRef.SetValue(AVToSet, SetToValue as Float)
        EndIf
    EndIf
EndEvent
