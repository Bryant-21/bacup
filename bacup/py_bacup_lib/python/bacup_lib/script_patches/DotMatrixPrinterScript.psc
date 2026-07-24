Event OnActivate(ObjectReference akActionRef)
    If RequiredKeywordOnPrinter != None && !Self.HasKeyword(RequiredKeywordOnPrinter)
        If DotMatrixPrinterMessageNotActive != None
            DotMatrixPrinterMessageNotActive.Show()
        EndIf
        Return
    EndIf
    If RequiredKeywordOnUser != None && !akActionRef.HasKeyword(RequiredKeywordOnUser)
        If DotMatrixPrinterMessageNotActive != None
            DotMatrixPrinterMessageNotActive.Show()
        EndIf
        Return
    EndIf

    Parent.OnActivate(akActionRef)
EndEvent
