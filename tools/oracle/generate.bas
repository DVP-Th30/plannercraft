Attribute VB_Name = "PlannerCraftOracle"
' =============================================================================
' PlannerCraft - generateur de l'oracle MS Project (ADR-0004)
'
' A executer dans Microsoft Project Desktop (module VBA importe via
' Visual Basic Editor > Fichier > Importer un fichier...).
'
' Pour chaque fichier MSPDI "*.xml" du dossier d'entree :
'   1. ouverture en lecture seule, comme nouveau projet ;
'   2. recalcul complet du planning par MS Project ;
'   3. enregistrement au format XML (MSPDI) dans le dossier de sortie, sous le
'      meme nom de fichier ;
'   4. fermeture sans enregistrer l'original.
' Toute erreur est consignee dans le journal "oracle.log" du dossier de sortie,
' et le traitement continue avec le fichier suivant.
'
' Disposition attendue par le harnais (voir tests/oracle/README.md) :
'   entree : tests/oracle/input/<cas>.xml
'   sortie : tests/oracle/msproject/<cas>.xml
'
' Ecrit d'apres la documentation publique du modele objet de Project
' (objet Application : FileOpenEx, CalculateProject, FileSaveAs, FileCloseEx).
' Pas d'accents dans ce fichier : l'editeur VBA lit les .bas en ANSI.
' =============================================================================
Option Explicit

' Format d'enregistrement XML de Project (MSPDI).
Private Const XML_FORMAT_ID As String = "MSProject.XML"
Private Const LOG_FILE_NAME As String = "oracle.log"

' Point d'entree : demande les deux dossiers puis lance le traitement.
Public Sub GenerateOracle()
    Dim inputDir As String
    Dim outputDir As String

    inputDir = InputBox("Dossier des fichiers MSPDI d'entree (tests/oracle/input) :", "PlannerCraft - oracle")
    If Len(inputDir) = 0 Then Exit Sub
    outputDir = InputBox("Dossier de sortie (tests/oracle/msproject) :", "PlannerCraft - oracle")
    If Len(outputDir) = 0 Then Exit Sub

    RunOracle inputDir, outputDir
End Sub

' Traite tous les fichiers *.xml de inputDir et ecrit les resultats dans outputDir.
' Utilisable directement depuis la fenetre Execution :
'   RunOracle "C:\plannercraft\tests\oracle\input", "C:\plannercraft\tests\oracle\msproject"
Public Sub RunOracle(ByVal inputDir As String, ByVal outputDir As String)
    Dim files As Collection
    Dim item As Variant
    Dim logNumber As Integer
    Dim errorMessage As String
    Dim okCount As Long
    Dim failedCount As Long
    Dim previousAlerts As Boolean

    inputDir = WithTrailingSeparator(inputDir)
    outputDir = WithTrailingSeparator(outputDir)

    If Len(Dir(inputDir, vbDirectory)) = 0 Then
        MsgBox "Dossier d'entree introuvable : " & inputDir, vbCritical, "PlannerCraft - oracle"
        Exit Sub
    End If
    If Len(Dir(outputDir, vbDirectory)) = 0 Then MkDir outputDir

    logNumber = FreeFile
    Open outputDir & LOG_FILE_NAME For Append As #logNumber
    WriteLog logNumber, "debut ; Project " & Application.Version & " build " & Application.Build
    WriteLog logNumber, "entree : " & inputDir
    WriteLog logNumber, "sortie : " & outputDir

    ' Liste figee avant traitement : FileOpenEx/FileSaveAs peuvent perturber Dir().
    Set files = ListXmlFiles(inputDir)
    WriteLog logNumber, files.Count & " fichier(s) a traiter"

    previousAlerts = Application.DisplayAlerts
    Application.DisplayAlerts = False

    For Each item In files
        errorMessage = ProcessFile(inputDir & CStr(item), outputDir & CStr(item))
        If Len(errorMessage) = 0 Then
            okCount = okCount + 1
            WriteLog logNumber, "OK     " & CStr(item)
        Else
            failedCount = failedCount + 1
            WriteLog logNumber, "ERREUR " & CStr(item) & " : " & errorMessage
        End If
    Next item

    Application.DisplayAlerts = previousAlerts

    WriteLog logNumber, "fin ; " & okCount & " reussi(s), " & failedCount & " en erreur"
    Close #logNumber

    MsgBox okCount & " fichier(s) exporte(s), " & failedCount & " en erreur." & vbCrLf & _
           "Journal : " & outputDir & LOG_FILE_NAME, vbInformation, "PlannerCraft - oracle"
End Sub

' Ouvre, recalcule, exporte et ferme un fichier. Renvoie "" en cas de succes,
' sinon le message d'erreur. Ne laisse jamais de projet ouvert derriere lui.
Private Function ProcessFile(ByVal inputPath As String, ByVal outputPath As String) As String
    Dim opened As Boolean
    Dim stepName As String

    On Error GoTo Failed

    stepName = "ouverture"
    ' Lecture seule, comme nouveau projet (pas de fusion avec le projet actif).
    If Not Application.FileOpenEx(Name:=inputPath, ReadOnly:=True, Merge:=pjDoNotMerge, _
                                  FormatID:=XML_FORMAT_ID) Then
        ProcessFile = "FileOpenEx a refuse le fichier"
        Exit Function
    End If
    opened = True

    stepName = "recalcul"
    ' Calcul automatique, puis recalcul explicite du projet actif.
    Application.Calculation = pjAutomatic
    Application.CalculateProject

    stepName = "enregistrement"
    If Len(Dir(outputPath)) > 0 Then Kill outputPath
    Application.FileSaveAs Name:=outputPath, FormatID:=XML_FORMAT_ID

    stepName = "fermeture"
    Application.FileCloseEx Save:=pjDoNotSave
    opened = False

    ProcessFile = ""
    Exit Function

Failed:
    ProcessFile = stepName & " : erreur " & Err.Number & " - " & Err.Description
    Err.Clear
    On Error Resume Next
    If opened Then Application.FileCloseEx Save:=pjDoNotSave
End Function

' Renvoie les noms (sans chemin) des fichiers *.xml de directory, tries.
Private Function ListXmlFiles(ByVal directory As String) As Collection
    Dim result As New Collection
    Dim fileNames() As String
    Dim fileCount As Long
    Dim fileName As String
    Dim i As Long
    Dim j As Long
    Dim current As String

    fileName = Dir(directory & "*.xml")
    Do While Len(fileName) > 0
        fileCount = fileCount + 1
        ReDim Preserve fileNames(1 To fileCount)
        fileNames(fileCount) = fileName
        fileName = Dir()
    Loop

    ' Tri par insertion : ordre de traitement deterministe dans le journal.
    For i = 2 To fileCount
        current = fileNames(i)
        j = i - 1
        Do While j >= 1
            If StrComp(fileNames(j), current, vbTextCompare) <= 0 Then Exit Do
            fileNames(j + 1) = fileNames(j)
            j = j - 1
        Loop
        fileNames(j + 1) = current
    Next i

    For i = 1 To fileCount
        result.Add fileNames(i)
    Next i
    Set ListXmlFiles = result
End Function

' Project Desktop ne tourne que sous Windows : separateur "\".
Private Function WithTrailingSeparator(ByVal dirPath As String) As String
    If Right$(dirPath, 1) = "\" Or Right$(dirPath, 1) = "/" Then
        WithTrailingSeparator = dirPath
    Else
        WithTrailingSeparator = dirPath & "\"
    End If
End Function

Private Sub WriteLog(ByVal logNumber As Integer, ByVal message As String)
    Print #logNumber, Format$(Now, "yyyy-mm-dd hh:nn:ss") & "  " & message
End Sub
