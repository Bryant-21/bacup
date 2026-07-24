Event OnInit()
    MoveToCountTo = 0
    If MoveRefsArray
        MoveToCountTo = MoveRefsArray.Length
    EndIf
    MoveToCounter = 0
    While MoveToCounter < MoveToCountTo
        If MoveRefsArray[MoveToCounter].RefToMove != None && MoveRefsArray[MoveToCounter].MoveBetweenRef1 != None && MoveRefsArray[MoveToCounter].MoveBetweenRef2 != None
            ObjectReference kMoveTarget = MoveRefsArray[MoveToCounter].RefToMove.GetReference()
            ObjectReference kRef1 = MoveRefsArray[MoveToCounter].MoveBetweenRef1.GetReference()
            ObjectReference kRef2 = MoveRefsArray[MoveToCounter].MoveBetweenRef2.GetReference()
            If kMoveTarget != None && kRef1 != None && kRef2 != None
                X1 = kRef1.GetPositionX()
                Y1 = kRef1.GetPositionY()
                Z1 = kRef1.GetPositionZ()
                X2 = kRef2.GetPositionX()
                Y2 = kRef2.GetPositionY()
                Z2 = kRef2.GetPositionZ()
                HowFarBetweenDif = MoveRefsArray[MoveToCounter].HowFarBetween
                kMoveTarget.SetPosition(X1 + (X2 - X1) * HowFarBetweenDif, Y1 + (Y2 - Y1) * HowFarBetweenDif, Z1 + (Z2 - Z1) * HowFarBetweenDif + zOffset)
            EndIf
        EndIf
        MoveToCounter += 1
    EndWhile

    RotateCountTo = 0
    If RotationRefs && LookAtRefs && RotationRefs.Length == LookAtRefs.Length
        RotateCountTo = RotationRefs.Length
    EndIf
    RotateCounter = 0
    While RotateCounter < RotateCountTo
        If RotationRefs[RotateCounter] != None && LookAtRefs[RotateCounter] != None
            ObjectReference kRotator = RotationRefs[RotateCounter].GetReference()
            ObjectReference kLookAt = LookAtRefs[RotateCounter].GetReference()
            If kRotator != None && kLookAt != None
                X1 = kRotator.GetPositionX()
                Y1 = kRotator.GetPositionY()
                X2 = kLookAt.GetPositionX()
                Y2 = kLookAt.GetPositionY()
                ; Papyrus has no atan2 - standard atan(dy/dx) plus quadrant
                ; correction derives the same yaw bearing (degrees) from
                ; kRotator to kLookAt.
                Float fBearing = 0.0
                If X2 - X1 != 0.0
                    fBearing = Math.atan((Y2 - Y1) / (X2 - X1))
                    If X2 - X1 < 0.0
                        fBearing += 180.0
                    ElseIf Y2 - Y1 < 0.0
                        fBearing += 360.0
                    EndIf
                ElseIf Y2 - Y1 >= 0.0
                    fBearing = 90.0
                Else
                    fBearing = 270.0
                EndIf
                kRotator.SetAngle(kRotator.GetAngleX(), kRotator.GetAngleY(), fBearing)
            EndIf
        EndIf
        RotateCounter += 1
    EndWhile

    SetStage(StageToSetAfterInit)
EndEvent
